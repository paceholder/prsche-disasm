//! Weather keys of the sky config `Sky/<track>.bin` (`docs/re/weather.md`).
//!
//! `SkyManager_Ctor` 0x45d800 builds three key lists from the raw config: the
//! sky-map blend (`+0xddc`, read by 0x45a350), the weather records (`+0x1a5c`,
//! 0x45a560 and its field readers 0x45a6a0..0x45a940) and a per-node flag
//! (`+0x655c`, 0x45a9e0). Keys are track node indices; `SkyManager_Draw`
//! 0x45d4f0 locates the camera's node every frame (`TrackNodes_Locate`
//! 0x481400) and every reader interpolates between the two keys around it,
//! wrapping at the node count.

use crate::{f32le, u32le, Result};

/// Keys per list (`0x45a2c0`/`0x45a490`/`0x45a970` stop at 400).
pub const WEATHER_KEYS: usize = 400;
/// Below this a fog density or the next key's density counts as zero
/// (`3e-06` in 0x45d2e0, 0x45d3b0, 0x45a720, 0x45a800).
pub const FOG_EPSILON: f32 = 3e-6;
/// Rain spray and the headlight flare need `rain > 0.1` (0x43ef80, 0x43cdd0).
pub const RAIN_THRESHOLD: f32 = 0.1;

/// One weather record (0x30 bytes at `+0x1a5c + 0x30·i`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WeatherKey {
    /// `+0x00`: track node of the key (negative ends the list).
    pub node: i32,
    /// `+0x04`: sky fog; density = value³ (0x45a6a0, used by 0x45d2e0 for the sky layers).
    pub sky_fog: f32,
    /// `+0x08`: world fog; density = value³ (0x45a6e0, used by 0x45d3b0 after the sky).
    pub world_fog: f32,
    /// `+0x10`: sky fog colour, ARGB (0x45a720).
    pub sky_fog_color: u32,
    /// `+0x14`: world fog colour, ARGB (0x45a800).
    pub world_fog_color: u32,
    /// `+0x18`: rain intensity (0x45a8e0 via 0x45d000).
    pub rain: f32,
    /// `+0x1c`: lightning intensity (0x45a910 via 0x45d030 → 0x45ccf0).
    pub lightning: f32,
    /// `+0x20`: snow intensity (0x45a940 via 0x45cfd0).
    pub snow: f32,
}

/// The interpolated weather at one node.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Weather {
    /// Sky fog density (0 when off) and its colour (ARGB).
    pub sky_fog_density: f32,
    pub sky_fog_color: u32,
    /// World fog density (0 when off) and its colour (ARGB).
    pub world_fog_density: f32,
    pub world_fog_color: u32,
    pub rain: f32,
    pub snow: f32,
    pub lightning: f32,
    /// Sky-map blend from region A (0) to region B (1) (`+0xddc`, 0x45a350).
    pub region_b: f32,
}

impl Weather {
    pub fn raining(&self) -> bool {
        self.rain > RAIN_THRESHOLD
    }
}

/// Clear colour record of a sky detail level (`+0xce8 + 0x3c·level`, 0x459950/0x459a70).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SkyClear {
    /// `+0x00`: record present.
    pub present: bool,
    /// `+0x04`: clear with the sky fog colour (0x45d2b0) instead of the colours below.
    pub use_fog_color: bool,
    /// `+0x08..+0x10` region A RGB, `+0x14..+0x1c` region B RGB (0..255 floats).
    pub color_a: [f32; 3],
    pub color_b: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkyWeather {
    /// `(node, blend)` keys of `+0xddc`.
    pub blend_keys: Vec<(i32, f32)>,
    pub keys: Vec<WeatherKey>,
    /// `(node, flag)` keys of `+0x655c` (read by 0x45d470 for the sun layer; all 0 in the shipped files).
    pub flag_keys: Vec<(i32, bool)>,
    /// Clear colour records of the four sky detail levels.
    pub clear: [SkyClear; 4],
}

/// `0x45a560` index search and weight: the keys `(i, next)` around `node`
/// and `w` such that a field is `w·key[i] + (1 − w)·key[next]`.
pub fn key_pair(nodes: &[i32], node: i32, node_count: i32) -> (usize, usize, f32) {
    let n = nodes.len();
    if n == 0 {
        return (0, 0, 1.0);
    }
    let next_of = |i: usize| if i + 1 >= n { 0 } else { i + 1 };
    let mut i = 0;
    let mut next = if n < 2 { 0 } else { 1 };
    while i + 1 < n {
        if nodes[i] <= node && node < nodes[next] {
            break;
        }
        i += 1;
        next = next_of(i);
    }
    if i == next || nodes[next] == nodes[i] {
        return (i, next, 1.0);
    }
    let wrap = |mut d: i32| {
        let half = node_count / 2;
        if d >= 1 && d > half {
            d -= node_count;
        } else if d < 0 && d < -half {
            d += node_count;
        }
        if d < 0 {
            d += node_count;
        }
        d
    };
    let span = wrap(nodes[next] - nodes[i]);
    let d = wrap(node - nodes[i]);
    if span == 0 {
        return (i, next, 1.0);
    }
    (i, next, 1.0 - d as f32 / span as f32)
}

/// `0x45a720`/`0x45a800`: per channel `(c_i·W + c_next·(1−W)) >> 16`, `W = round(w·65536)`,
/// alpha 0xff.
fn lerp_argb(a: u32, b: u32, w: f32) -> u32 {
    let wa = (w * 65536.0).round() as u32;
    let wb = 0x10000u32.wrapping_sub(wa);
    let ch = |s: u32| ((((a >> s) & 0xff) * wa) >> 16) + ((((b >> s) & 0xff) * wb) >> 16);
    0xff00_0000 | (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

impl SkyWeather {
    /// The weather at track node `node` of a track with `node_count` nodes.
    pub fn at(&self, node: i32, node_count: i32) -> Weather {
        let mut out = Weather::default();
        if !self.keys.is_empty() {
            let nodes: Vec<i32> = self.keys.iter().map(|k| k.node).collect();
            let (i, j, w) = key_pair(&nodes, node, node_count);
            let (a, b) = (&self.keys[i], &self.keys[j]);
            let lerp = |x: f32, y: f32| w * x + (1.0 - w) * y;
            // 0x45a6a0 / 0x45a6e0: the cube of the interpolated value.
            let sky = lerp(a.sky_fog, b.sky_fog).powi(3);
            let world = lerp(a.world_fog, b.world_fog).powi(3);
            if sky > FOG_EPSILON {
                out.sky_fog_density = sky;
                // 0x45a720(w, 0): the current key's colour when the next key has no sky fog.
                out.sky_fog_color = if b.sky_fog <= FOG_EPSILON {
                    a.sky_fog_color
                } else {
                    lerp_argb(a.sky_fog_color, b.sky_fog_color, w)
                };
            }
            if world > FOG_EPSILON {
                out.world_fog_density = world;
                out.world_fog_color = if FOG_EPSILON < b.world_fog {
                    lerp_argb(a.world_fog_color, b.world_fog_color, w)
                } else {
                    a.world_fog_color
                };
            }
            out.rain = lerp(a.rain, b.rain);
            out.lightning = lerp(a.lightning, b.lightning);
            out.snow = lerp(a.snow, b.snow);
        }
        if !self.blend_keys.is_empty() {
            let nodes: Vec<i32> = self.blend_keys.iter().map(|k| k.0).collect();
            let (i, j, w) = key_pair(&nodes, node, node_count);
            // 0x45a350 returns t·v_next + (1 − t)·v_i with t = 1 − w.
            out.region_b = w * self.blend_keys[i].1 + (1.0 - w) * self.blend_keys[j].1;
        }
        out
    }

    /// `0x45a720(w, 1)` (0x45d2b0): the sky fog colour lerp even when the next key has no fog.
    pub fn sky_fog_color_forced(&self, node: i32, node_count: i32) -> u32 {
        if self.keys.is_empty() {
            return 0xff00_0000;
        }
        let nodes: Vec<i32> = self.keys.iter().map(|k| k.node).collect();
        let (i, j, w) = key_pair(&nodes, node, node_count);
        lerp_argb(self.keys[i].sky_fog_color, self.keys[j].sky_fog_color, w)
    }

    /// Clear colour record for sky detail `level` (`0x459950`: the highest present record
    /// at or below `level`, else record 0).
    pub fn clear_record(&self, level: usize) -> SkyClear {
        (0..=level.min(3)).rev().map(|l| self.clear[l]).find(|c| c.present).unwrap_or(self.clear[0])
    }
}

fn i32le(d: &[u8], o: usize) -> Result<i32> {
    u32le(d, o).map(|v| v as i32)
}

/// The key lists of a version-9 sky config (older versions upgrade to two default keys).
pub fn parse_sky_weather(data: &[u8]) -> Result<SkyWeather> {
    let mut w = SkyWeather::default();
    for i in 0..WEATHER_KEYS {
        let o = 0xddc + i * 8;
        let node = i32le(data, o)?;
        if node < 0 {
            break;
        }
        w.blend_keys.push((node, f32le(data, o + 4)?));
    }
    for i in 0..WEATHER_KEYS {
        let o = 0x1a5c + i * 0x30;
        let node = i32le(data, o)?;
        if node < 0 {
            break;
        }
        w.keys.push(WeatherKey {
            node,
            sky_fog: f32le(data, o + 4)?,
            world_fog: f32le(data, o + 8)?,
            sky_fog_color: u32le(data, o + 0x10)?,
            world_fog_color: u32le(data, o + 0x14)?,
            rain: f32le(data, o + 0x18)?,
            lightning: f32le(data, o + 0x1c)?,
            snow: f32le(data, o + 0x20)?,
        });
    }
    for i in 0..WEATHER_KEYS {
        let o = 0x655c + i * 8;
        let node = i32le(data, o)?;
        if node < 0 {
            break;
        }
        w.flag_keys.push((node, data.get(o + 4).copied().unwrap_or(0) != 0));
    }
    for (l, c) in w.clear.iter_mut().enumerate() {
        let o = 0xce8 + l * 0x3c;
        let f = |k: usize| f32le(data, o + 8 + 4 * k);
        *c = SkyClear {
            present: u32le(data, o)? != 0,
            use_fog_color: u32le(data, o + 4)? != 0,
            color_a: [f(0)?, f(1)?, f(2)?],
            color_b: [f(3)?, f(4)?, f(5)?],
        };
    }
    // 0x45a2c0 / 0x45a490: fewer than two keys become two default keys.
    if w.blend_keys.len() < 2 {
        w.blend_keys = vec![(0, 0.0), (4, 1.0)];
    }
    if w.keys.len() < 2 {
        w.keys = vec![WeatherKey::default(), WeatherKey { node: 4, ..Default::default() }];
    }
    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_inside_and_wrapping() {
        let nodes = [0, 100, 200];
        assert_eq!(key_pair(&nodes, 50, 300), (0, 1, 0.5));
        assert_eq!(key_pair(&nodes, 100, 300), (1, 2, 1.0));
        // Past the last key: towards key 0 across the lap end (span 100).
        let (i, j, w) = key_pair(&nodes, 250, 300);
        assert_eq!((i, j), (2, 0));
        assert!((w - 0.5).abs() < 1e-6);
    }

    #[test]
    fn argb_lerp_matches_fixed_point() {
        assert_eq!(lerp_argb(0xff00_0000, 0xff00_00ff, 1.0), 0xff00_0000);
        assert_eq!(lerp_argb(0xff00_00ff, 0xff00_0000, 0.5), 0xff00_007f);
    }

    fn config(keys: &[WeatherKey]) -> Vec<u8> {
        let mut d = vec![0u8; crate::sky::SKY_CONFIG_SIZE];
        let put = |d: &mut Vec<u8>, o: usize, v: u32| d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        for list in [0xddc, 0x655c] {
            put(&mut d, list, 0);
            put(&mut d, list + 8, 4);
            put(&mut d, list + 16, u32::MAX);
        }
        for (i, k) in keys.iter().enumerate() {
            let o = 0x1a5c + i * 0x30;
            put(&mut d, o, k.node as u32);
            put(&mut d, o + 4, k.sky_fog.to_bits());
            put(&mut d, o + 8, k.world_fog.to_bits());
            put(&mut d, o + 0x10, k.sky_fog_color);
            put(&mut d, o + 0x14, k.world_fog_color);
            put(&mut d, o + 0x18, k.rain.to_bits());
            put(&mut d, o + 0x20, k.snow.to_bits());
        }
        put(&mut d, 0x1a5c + keys.len() * 0x30, u32::MAX);
        d
    }

    #[test]
    fn fog_is_cubed_and_snow_interpolated() {
        let a = WeatherKey { node: 0, world_fog: 0.2, world_fog_color: 0xff10_2030, ..Default::default() };
        let b = WeatherKey { node: 10, world_fog: 0.2, world_fog_color: 0xff10_2030, snow: 1.0, ..Default::default() };
        let w = parse_sky_weather(&config(&[a, b])).unwrap();
        let at = w.at(5, 100);
        assert!((at.world_fog_density - 0.008).abs() < 1e-6);
        assert_eq!(at.world_fog_color, 0xff10_2030);
        assert!((at.snow - 0.5).abs() < 1e-6);
        assert_eq!(at.sky_fog_density, 0.0);
    }
}

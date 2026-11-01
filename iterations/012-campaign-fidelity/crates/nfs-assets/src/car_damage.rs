//! Damage morph of the car model: the ten damage zones (`car+0x4e4`) move the vertices of the
//! CRP parts that carry a damage table.
//!
//! - `R3DCar_UpdateDamage` 0x438230 compares the capped zones with the cache `R3DCar+0x6a8`
//!   (zeroed by the constructor 0x437920) and, when one changed, runs the morph of every LOD,
//!   stores the new cache and updates the damage textures (0x4451e0, not here).
//! - `R3DCar_LightCap` 0x4382e0: the zone as the model sees it, `min(zone, cap * 100)`; the
//!   viewed car uses the cap table 0x5b35d4 when the camera id is not 0 or the in-car flag
//!   (`[0x627f43] & 8`) is set, every other car the table 0x5b35ac (all 1).
//! - `R3DCarLOD_ApplyDamageMorph` 0x43e390, per part with `Base+0x44+0x0a & 8`, at car detail
//!   `[0x6197c4]` above 0: the damage table is the article child `zd` (id `0x647a0000 | level`):
//!   `u32 n`, `n` triples `(zone, vertex count, first)`, then the vertex index list. The base
//!   array is `vt` index `level`, the delta array `vt` index `level | 0x8000`. For each triple
//!   whose zone changed: `t = zone * 0.02` (f32), above 1 clamped to 1; every listed vertex
//!   becomes `base + t * delta` (x, y, z; w = 1). The part's byte `+0x50` is cleared first and set
//!   when a changed zone has `t` above 0.5. A part with a table but no delta array is restored
//!   from its base array.
//!
//! Recorded: `scripts/research/replay-car-deform.py` runs 0x438230 with its callees unchanged
//! in Unicorn; `tests/original_car_deform.rs` compares every step bit for bit.
use nfs_formats::Entry;

/// Cap table of every car (`0x5b35ac`).
pub const CAP_TABLE: [f32; 10] = [1.0; 10];
/// Cap table of the viewed car (`0x5b35d4`): zones 3, 7 and 8 never reach its model.
pub const CAP_TABLE_VIEWED: [f32; 10] = [1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.0];
/// `[0x5b3714]`.
const ZONE_TO_T: f32 = 0.02;

/// What 0x4382e0 reads besides the zones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DamageView {
    /// `car+0x520 == 0x465d80()`: the car is the one the camera follows.
    pub viewed: bool,
    /// `Camera_GetId(0)` 0x465770, `[0x627f3c]`.
    pub camera_id: i16,
    /// `[0x627f43] & 8` (set by `Camera_Set` 0x465790 for camera ids below 2).
    pub in_car: bool,
}

impl DamageView {
    fn caps(self) -> &'static [f32; 10] {
        if self.viewed && (self.camera_id != 0 || self.in_car) {
            &CAP_TABLE_VIEWED
        } else {
            &CAP_TABLE
        }
    }
}

/// `R3DCar_LightCap` 0x4382e0: zone `i` as the model sees it.
pub fn capped_zone(zones: &[f32; 10], i: usize, view: DamageView) -> f32 {
    let cap = view.caps()[i] * 100.0;
    if cap < zones[i] {
        cap
    } else {
        zones[i]
    }
}

/// The `zd` table of one part level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamageTable {
    /// `(zone, vertex count, first index)`.
    pub triples: Vec<[u32; 3]>,
    /// Vertex indices (into the part's `vt` array) listed by the triples.
    pub indices: Vec<u32>,
}

impl DamageTable {
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        let word = |k: usize| -> Result<u32, String> {
            data.get(4 * k..4 * k + 4)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
                .ok_or_else(|| format!("zd: truncated at word {k}"))
        };
        let n = word(0)? as usize;
        if n > 64 {
            return Err(format!("zd: {n} triples"));
        }
        let triples = (0..n)
            .map(|t| Ok([word(1 + 3 * t)?, word(2 + 3 * t)?, word(3 + 3 * t)?]))
            .collect::<Result<Vec<_>, String>>()?;
        let len = triples
            .iter()
            .map(|t| t[1] as usize + t[2] as usize)
            .max()
            .unwrap_or(0);
        let indices = (0..len)
            .map(|k| word(1 + 3 * n + k))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { triples, indices })
    }
}

fn vec4s(entry: &Entry) -> Result<Vec<[f32; 4]>, String> {
    let n = entry.count;
    let raw = entry.data.get(..16 * n).ok_or_else(|| {
        format!(
            "vt {:#x}: {} vertices in {} bytes",
            entry.index,
            n,
            entry.data.len()
        )
    })?;
    Ok(raw
        .as_chunks::<16>()
        .0
        .iter()
        .map(|v| {
            std::array::from_fn(|k| f32::from_le_bytes(v[4 * k..4 * k + 4].try_into().unwrap()))
        })
        .collect())
}

/// One part record of an `R3DCarLOD` (0x54 bytes in the original), as far as the morph uses it.
#[derive(Debug, Clone)]
pub struct MorphPart {
    /// `Base+0x44+0x0a`: bit 3 enables the morph.
    pub flags: u8,
    pub level: u16,
    /// `vt` index `level`.
    pub base: Vec<[f32; 4]>,
    /// `vt` index `level | 0x8000`.
    pub delta: Option<Vec<[f32; 4]>>,
    /// `zd` index `level`.
    pub table: Option<DamageTable>,
    /// The drawn vertex array (`part+0x40`), x, y, z, w in raw CRP units.
    pub vertices: Vec<[f32; 4]>,
    /// `part+0x50`.
    pub heavy: bool,
}

impl MorphPart {
    /// The part of `article` at `level`; `None` when the article has no `vt` at that level.
    pub fn from_article(article: &Entry, level: u16) -> Result<Option<Self>, String> {
        let Some(vt) = article.find("vt", level) else {
            return Ok(None);
        };
        let base = vec4s(vt)?;
        let flags = article
            .find("Base", 0)
            .and_then(|b| b.data.get(0x44 + 0x0a).copied())
            .ok_or("article has no Base flags")?;
        let delta = article.find("vt", level | 0x8000).map(vec4s).transpose()?;
        let table = article
            .find("dz", level)
            .map(|e| DamageTable::parse(&e.data))
            .transpose()?;
        if let (Some(table), Some(delta)) = (&table, &delta) {
            let n = base.len().min(delta.len());
            if table.indices.iter().any(|&i| i as usize >= n) {
                return Err(format!("zd level {level}: vertex index out of range"));
            }
        }
        Ok(Some(Self {
            flags,
            level,
            vertices: base.clone(),
            base,
            delta,
            table,
            heavy: false,
        }))
    }

    /// Whether the morph can ever move this part.
    pub fn morphs(&self) -> bool {
        self.flags & 8 != 0 && self.table.is_some()
    }

    /// 0x43e390 for this part. `zone(i)` is the capped zone, `changed(i)` the 0x438390 test.
    /// Returns whether the vertex array was written.
    fn apply(&mut self, zone: impl Fn(usize) -> f32, changed: impl Fn(usize) -> bool) -> bool {
        if self.flags & 8 == 0 {
            return false;
        }
        self.heavy = false;
        let Some(table) = &self.table else {
            return false;
        };
        let Some(delta) = &self.delta else {
            self.vertices.copy_from_slice(&self.base);
            return true;
        };
        let mut wrote = false;
        for &[z, count, first] in &table.triples {
            let z = z as usize;
            // Zone numbers come from the file; 0x4382e0 would read past the ten zones.
            if z >= 10 || !changed(z) {
                continue;
            }
            let mut t = zone(z) * ZONE_TO_T;
            if t > 1.0 {
                t = 1.0;
                self.heavy = true;
            } else if t > 0.5 {
                self.heavy = true;
            }
            for j in 0..count as usize {
                let v = table.indices[first as usize + j] as usize;
                let (b, d) = (self.base[v], delta[v]);
                self.vertices[v] = [d[0] * t + b[0], d[1] * t + b[1], d[2] * t + b[2], 1.0];
                wrote = true;
            }
        }
        wrote
    }
}

/// The damage state of one drawn car: the zone cache and its morphable parts.
#[derive(Debug, Clone, Default)]
pub struct CarDamageModel {
    /// `R3DCar+0x6a8`.
    pub cache: [f32; 10],
    pub parts: Vec<MorphPart>,
}

impl CarDamageModel {
    /// Parts of every article at `level` (one `R3DCarLOD`).
    pub fn new(parts: Vec<MorphPart>) -> Self {
        Self {
            cache: [0.0; 10],
            parts,
        }
    }

    /// `R3DCar_UpdateDamage` 0x438230. `detail` is the car detail `[0x6197c4]`. `None` when
    /// no capped zone changed (and `force` is false); else the indices of the parts whose
    /// vertex array was written (the original then also updates the damage textures).
    pub fn update(
        &mut self,
        zones: &[f32; 10],
        view: DamageView,
        detail: i32,
        force: bool,
    ) -> Option<Vec<usize>> {
        let capped: [f32; 10] = std::array::from_fn(|i| capped_zone(zones, i, view));
        if !force && capped == self.cache {
            return None;
        }
        let mut written = Vec::new();
        if detail > 0 {
            let cache = self.cache;
            for (k, part) in self.parts.iter_mut().enumerate() {
                if part.apply(|i| capped[i], |i| capped[i] != cache[i]) {
                    written.push(k);
                }
            }
        }
        self.cache = capped;
        Some(written)
    }

    /// Back to the undamaged model (a new `R3DCar`: cache 0, arrays copied from the base).
    pub fn reset(&mut self) {
        self.cache = [0.0; 10];
        for part in &mut self.parts {
            part.vertices.copy_from_slice(&part.base);
            part.heavy = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewed_car_caps_side_and_roof() {
        let mut zones = [0.0; 10];
        zones[8] = 80.0;
        zones[3] = 120.0;
        let viewed = DamageView {
            viewed: true,
            camera_id: 3,
            in_car: false,
        };
        assert_eq!(capped_zone(&zones, 8, viewed), 0.0);
        assert_eq!(capped_zone(&zones, 3, viewed), 0.0);
        let bumper = DamageView {
            viewed: true,
            camera_id: 0,
            in_car: false,
        };
        assert_eq!(capped_zone(&zones, 8, bumper), 80.0);
        assert_eq!(capped_zone(&zones, 3, DamageView::default()), 100.0);
    }

    #[test]
    fn zone_fifty_is_the_full_delta() {
        let mut part = MorphPart {
            flags: 8,
            level: 1,
            base: vec![[1.0, 2.0, 3.0, 1.0]; 2],
            delta: Some(vec![[0.5, -1.0, 2.0, 0.3]; 2]),
            table: Some(DamageTable {
                triples: vec![[4, 1, 0]],
                indices: vec![1],
            }),
            vertices: vec![[1.0, 2.0, 3.0, 1.0]; 2],
            heavy: false,
        };
        part.vertices[0][3] = 0.25;
        let mut model = CarDamageModel::new(vec![part]);
        let mut zones = [0.0; 10];
        zones[4] = 50.0;
        assert_eq!(
            model.update(&zones, DamageView::default(), 2, false),
            Some(vec![0])
        );
        let p = &model.parts[0];
        assert_eq!(p.vertices[1], [1.5, 1.0, 5.0, 1.0]);
        assert_eq!(
            p.vertices[0],
            [1.0, 2.0, 3.0, 0.25],
            "unlisted vertex untouched"
        );
        assert!(p.heavy);
        assert_eq!(
            model.update(&zones, DamageView::default(), 2, false),
            None,
            "cache hit"
        );
    }
}

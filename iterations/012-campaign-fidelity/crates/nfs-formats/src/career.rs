//! NFS5 career data formats: profile saves (.sav), car catalog (nfs5.car),
//! track catalog (nfs5.trk), and factory driver missions (nfs5.fac).
use crate::{bytes, u32le, Result};

pub const FACTORY_MISSION_REC_SIZE: usize = 240;
pub const FACTORY_MISSION_COUNT: usize = 34;
pub const CAR_CATALOG_REC_SIZE: usize = 1648;
pub const TRACK_CATALOG_REC_SIZE: usize = 1192;
pub const TRACK_CATALOG_COUNT: usize = 15;
pub const TOURNAMENT_REC_SIZE: usize = 3392;
pub const TOURNAMENT_COUNT: usize = 35;
pub const PARTS_CATALOG_REC_SIZE: usize = 360;
pub const PARTS_CATALOG_COUNT: usize = 683;

/// Known top-level section in a profile save file (.sav).
#[derive(Debug, Clone, PartialEq)]
pub struct SaveSection {
    pub tag: String,
    pub offset: usize,
    pub payload: Vec<u8>,
}

/// Parsed player profile save file (.sav).
#[derive(Debug, Clone, PartialEq)]
pub struct SaveFile {
    /// Identified modular sections in the save file.
    pub sections: Vec<SaveSection>,
    /// Player cash / credits from PlayerInfo.
    pub cash: u32,
    /// Number of cars owned in player's garage.
    pub owned_cars_count: u32,
    /// Factory driver mission IDs parsed from Missions section.
    pub mission_codes: Vec<String>,
}

impl SaveFile {
    /// Find a section by its ASCII tag name.
    pub fn get_section(&self, tag: &str) -> Option<&[u8]> {
        self.sections
            .iter()
            .find(|s| s.tag == tag)
            .map(|s| s.payload.as_slice())
    }
}

/// Known registered section tags in nfs5.exe save linked list.
const REGISTERED_SAVE_TAGS: &[&str] = &[
    "Missions",
    "PartsMarket",
    "Tournament",
    "PlayerInfo",
    "TrackRecords",
    "UsedCarMarket",
    "Personalities",
    "Knockout",
    "CDPlayer",
];

/// Parse a profile `.sav` file into modular sections and player profile data.
pub fn parse_save(data: &[u8]) -> Result<SaveFile> {
    if data.len() < 32 {
        return Err(format!("save file too small: {} bytes", data.len()));
    }

    let mut found_sections = Vec::new();
    for &tag_name in REGISTERED_SAVE_TAGS {
        let tag_bytes = format!("{tag_name}\0").into_bytes();
        let mut pos = 0;
        while pos + tag_bytes.len() <= data.len() {
            if let Some(idx) = data[pos..]
                .windows(tag_bytes.len())
                .position(|w| w == tag_bytes)
            {
                let abs_idx = pos + idx;
                found_sections.push((abs_idx, tag_name.to_string(), tag_bytes.len()));
                pos = abs_idx + tag_bytes.len();
            } else {
                break;
            }
        }
    }

    found_sections.sort_by_key(|&(offset, _, _)| offset);

    let mut sections = Vec::new();
    for i in 0..found_sections.len() {
        let (start_offset, ref tag, header_len) = found_sections[i];
        let next_start = if i + 1 < found_sections.len() {
            found_sections[i + 1].0
        } else {
            data.len()
        };
        let payload_start = start_offset + header_len;
        let payload = if payload_start <= next_start {
            data[payload_start..next_start].to_vec()
        } else {
            Vec::new()
        };
        sections.push(SaveSection {
            tag: tag.clone(),
            offset: start_offset,
            payload,
        });
    }

    // Extract player credits and car count from PlayerInfo
    let mut cash = 0u32;
    let mut owned_cars_count = 0u32;
    if let Some(pi) = sections.iter().find(|s| s.tag == "PlayerInfo") {
        if pi.payload.len() >= 8 {
            owned_cars_count = u32le(&pi.payload, 0).unwrap_or(0);
            cash = u32le(&pi.payload, 4).unwrap_or(0);
        }
    }

    // Extract factory driver mission codes from Missions section
    let mut mission_codes = Vec::new();
    if let Some(m_sec) = sections.iter().find(|s| s.tag == "Missions") {
        // Payload starts with 4-byte size (8160) + header, missions start at offset 67
        for chunk in m_sec
            .payload
            .windows(FACTORY_MISSION_REC_SIZE)
            .step_by(FACTORY_MISSION_REC_SIZE)
        {
            // Find [0-9][mM][0-9]{2} in chunk
            for w in chunk.windows(4) {
                if (w[0].is_ascii_digit())
                    && (w[1] == b'm' || w[1] == b'M')
                    && (w[2].is_ascii_digit())
                    && (w[3].is_ascii_digit())
                {
                    if let Ok(code) = std::str::from_utf8(w) {
                        mission_codes.push(code.to_string());
                        break;
                    }
                }
            }
        }
    }

    Ok(SaveFile {
        sections,
        cash,
        owned_cars_count,
        mission_codes,
    })
}

/// Car entry from master vehicle specification catalog (`nfs5.car`).
#[derive(Debug, Clone, PartialEq)]
pub struct CarCatalogEntry {
    /// Human-readable car name (e.g. `'50 356 1100 Cabriolet`).
    pub display_name: String,
    /// Spec code identifier (e.g. `1950356Cabrio11`).
    pub spec_code: String,
    /// 3D model CRP name (e.g. `356_1`).
    pub model_name: String,
    /// Physics simulation file name without extension (e.g. `356cabrio11`).
    pub sim_name: String,
    /// Body style name (e.g. `Cabrio`).
    pub body_style: String,
}

/// Parse the master car catalog (`nfs5.car`).
pub fn parse_car_catalog(data: &[u8]) -> Result<Vec<CarCatalogEntry>> {
    if !data.len().is_multiple_of(CAR_CATALOG_REC_SIZE) {
        return Err(format!(
            "invalid nfs5.car size: {} is not a multiple of {CAR_CATALOG_REC_SIZE}",
            data.len()
        ));
    }

    let count = data.len() / CAR_CATALOG_REC_SIZE;
    let mut entries = Vec::with_capacity(count);

    for i in 0..count {
        let rec = bytes(data, i * CAR_CATALOG_REC_SIZE, CAR_CATALOG_REC_SIZE)?;

        fn extract_str(slice: &[u8]) -> String {
            let start = match slice.iter().position(|&b| b.is_ascii_graphic()) {
                Some(p) => p,
                None => return String::new(),
            };
            let end = slice[start..]
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(slice.len() - start);
            String::from_utf8_lossy(&slice[start..start + end])
                .trim()
                .to_string()
        }

        let display_name = extract_str(&rec[0x000..0x034]);
        let spec_code = extract_str(&rec[0x048..0x07c]);
        let model_name = extract_str(&rec[0x07c..0x0b0]);
        let sim_name = extract_str(&rec[0x0b0..0x0e2]);
        let body_style = extract_str(&rec[0x1b0..0x1c2]);

        entries.push(CarCatalogEntry {
            display_name,
            spec_code,
            model_name,
            sim_name,
            body_style,
        });
    }

    Ok(entries)
}

/// Track entry from track catalog (`nfs5.trk`).
#[derive(Debug, Clone, PartialEq)]
pub struct TrackCatalogEntry {
    /// Internal track name (e.g. `coastal`, `skidpad`).
    pub internal_name: String,
    /// Track location description (e.g. `Alberta`).
    pub location: String,
    /// i16 at +0x00 (track id).
    pub id: i32,
    /// i32 at +0x04: string id 300..315.
    pub name_string: i32,
    /// Photo FSH pack name (C string at +0x44), e.g. `AlpsSp`.
    pub photo_pack: String,
    /// Small photo pack (C string at +0x58), e.g. `AlpsSpSm`.
    pub small_pack: String,
    /// i16 at +0x6c: 3 = needs career progress.
    pub unlock: i32,
    /// i16 at +0x6e == 1: circuit track.
    pub circuit: bool,
    /// i32 at +0x78: 0..2.
    pub difficulty: i32,
}

/// JSON array of the track table for the web front end.
pub fn track_catalog_json(entries: &[TrackCatalogEntry]) -> String {
    fn q(s: &str) -> String {
        let mut o = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
                c => o.push(c),
            }
        }
        o.push('"');
        o
    }
    let v: Vec<String> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            format!(
                "{{\"index\":{i},\"internal_name\":{},\"location\":{},\"id\":{},\"name_string\":{},\"photo_pack\":{},\"small_pack\":{},\"unlock\":{},\"circuit\":{},\"difficulty\":{}}}",
                q(&e.internal_name),
                q(&e.location),
                e.id,
                e.name_string,
                q(&e.photo_pack),
                q(&e.small_pack),
                e.unlock,
                e.circuit,
                e.difficulty
            )
        })
        .collect();
    format!("[{}]", v.join(","))
}

/// Parse the track catalog (`nfs5.trk`).
pub fn parse_track_catalog(data: &[u8]) -> Result<Vec<TrackCatalogEntry>> {
    if !data.len().is_multiple_of(TRACK_CATALOG_REC_SIZE) {
        return Err(format!(
            "invalid nfs5.trk size: {} is not a multiple of {TRACK_CATALOG_REC_SIZE}",
            data.len()
        ));
    }

    let count = data.len() / TRACK_CATALOG_REC_SIZE;
    let mut entries = Vec::with_capacity(count);

    for i in 0..count {
        let rec = bytes(data, i * TRACK_CATALOG_REC_SIZE, TRACK_CATALOG_REC_SIZE)?;

        fn extract_str(slice: &[u8]) -> String {
            let start = match slice.iter().position(|&b| b.is_ascii_graphic()) {
                Some(p) => p,
                None => return String::new(),
            };
            let end = slice[start..]
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(slice.len() - start);
            String::from_utf8_lossy(&slice[start..start + end])
                .trim()
                .to_string()
        }

        let internal_name = extract_str(&rec[0x008..0x030]);
        let location = extract_str(&rec[0x1b0..0x1c4]);

        entries.push(TrackCatalogEntry {
            internal_name,
            location,
            id: i16::from_le_bytes([rec[0], rec[1]]) as i32,
            name_string: u32le(rec, 4)? as i32,
            photo_pack: extract_str(&rec[0x44..0x58]),
            small_pack: extract_str(&rec[0x58..0x6c]),
            unlock: i16::from_le_bytes([rec[0x6c], rec[0x6d]]) as i32,
            circuit: i16::from_le_bytes([rec[0x6e], rec[0x6f]]) == 1,
            difficulty: u32le(rec, 0x78)? as i32,
        });
    }

    Ok(entries)
}

/// Factory driver mission template from `nfs5.fac`.
#[derive(Debug, Clone, PartialEq)]
pub struct FactoryMissionTemplate {
    /// Mission identifier (e.g. `0M01`, `1m01`).
    pub code: String,
    /// Numerical mission ID.
    pub id: u32,
    /// Briefing string ID from festrings.csv.
    pub brief_string_id: u32,
    /// Time limit in seconds.
    pub time_limit_sec: u32,
    /// Track catalog index (e.g. 13 for skidpad).
    pub track_id: u32,
}

/// Parse factory driver mission templates (`nfs5.fac`).
pub fn parse_factory_missions(data: &[u8]) -> Result<Vec<FactoryMissionTemplate>> {
    if !data.len().is_multiple_of(FACTORY_MISSION_REC_SIZE) {
        return Err(format!(
            "invalid nfs5.fac size: {} is not a multiple of {FACTORY_MISSION_REC_SIZE}",
            data.len()
        ));
    }

    let count = data.len() / FACTORY_MISSION_REC_SIZE;
    let mut missions = Vec::with_capacity(count);

    for i in 0..count {
        let rec = bytes(data, i * FACTORY_MISSION_REC_SIZE, FACTORY_MISSION_REC_SIZE)?;
        let id = u32le(rec, 0)?;
        let brief_string_id = u32le(rec, 0x00)?;
        let time_limit_sec = u32le(rec, 0x20)?;
        let track_id = if rec.len() > 0x73 {
            rec[0x72] as u32 | ((rec[0x73] as u32) << 8)
        } else {
            0
        };

        let mut code = String::new();
        for w in rec.windows(4) {
            if (w[0].is_ascii_digit())
                && (w[1] == b'm' || w[1] == b'M')
                && (w[2].is_ascii_digit())
                && (w[3].is_ascii_digit())
            {
                if let Ok(c) = std::str::from_utf8(w) {
                    code = c.to_string();
                    break;
                }
            }
        }

        missions.push(FactoryMissionTemplate {
            code,
            id,
            brief_string_id,
            time_limit_sec,
            track_id,
        });
    }

    Ok(missions)
}

/// A stage / race in an Evolution tournament.
#[derive(Debug, Clone, PartialEq)]
pub struct TournamentStage {
    pub track_id: u32,
    pub prizes: [u32; 8],
    pub entry_fee: u32,
}

/// Evolution tournament record from `nfs5.trn`.
#[derive(Debug, Clone, PartialEq)]
pub struct TournamentRecord {
    pub title_string_id: u32,
    pub desc_string_id: u32,
    pub num_stages: u32,
    pub entry_fee: u32,
    pub stages: Vec<TournamentStage>,
}

/// Parse tournaments from `nfs5.trn`.
pub fn parse_tournaments(data: &[u8]) -> Result<Vec<TournamentRecord>> {
    if !data.len().is_multiple_of(TOURNAMENT_REC_SIZE) {
        return Err(format!(
            "invalid nfs5.trn size: {} is not a multiple of {TOURNAMENT_REC_SIZE}",
            data.len()
        ));
    }

    let count = data.len() / TOURNAMENT_REC_SIZE;
    let mut tournaments = Vec::with_capacity(count);

    for i in 0..count {
        let rec = bytes(data, i * TOURNAMENT_REC_SIZE, TOURNAMENT_REC_SIZE)?;
        let title_string_id = u32le(rec, 0x24)?;
        let desc_string_id = u32le(rec, 0x28)?;
        let num_stages = u32le(rec, 0x39c)?;
        let entry_fee = u32le(rec, 0x3c8)?;

        let mut stages = Vec::new();
        let stage_count = (num_stages as usize).min(8);
        for s in 0..stage_count {
            let offset = 0x3a0 + s * 0xa4;
            if offset + 0x30 <= rec.len() {
                let track_id = u32le(rec, offset)?;
                let mut prizes = [0u32; 8];
                for (p, prize) in prizes.iter_mut().enumerate() {
                    *prize = u32le(rec, offset + 8 + p * 4)?;
                }
                let s_fee = u32le(rec, offset + 0x28)?;
                stages.push(TournamentStage {
                    track_id,
                    prizes,
                    entry_fee: s_fee,
                });
            }
        }

        tournaments.push(TournamentRecord {
            title_string_id,
            desc_string_id,
            num_stages,
            entry_fee,
            stages,
        });
    }

    Ok(tournaments)
}

/// A tuning or replacement part record from `nfs5.prt`.
#[derive(Debug, Clone, PartialEq)]
pub struct PartRecord {
    pub id: u32,
    pub name_string_id: u32,
    pub car_model: String,
    pub category: String,
    pub price: u32,
    pub base_condition: f32,
    pub modifier_value: f32,
    pub secondary_value: f32,
    pub torque_curve: Vec<f32>,
}

/// Parse parts catalog from `nfs5.prt`.
pub fn parse_parts_catalog(data: &[u8]) -> Result<Vec<PartRecord>> {
    if !data.len().is_multiple_of(PARTS_CATALOG_REC_SIZE) {
        return Err(format!(
            "invalid nfs5.prt size: {} is not a multiple of {PARTS_CATALOG_REC_SIZE}",
            data.len()
        ));
    }

    let count = data.len() / PARTS_CATALOG_REC_SIZE;
    let mut parts = Vec::with_capacity(count);

    for i in 0..count {
        let rec = bytes(data, i * PARTS_CATALOG_REC_SIZE, PARTS_CATALOG_REC_SIZE)?;
        let id = u32le(rec, 0)?;
        let name_string_id = u32le(rec, 4)?;

        let car_raw = &rec[8..40];
        let car_len = car_raw
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(car_raw.len());
        let car_model = String::from_utf8_lossy(&car_raw[..car_len])
            .trim()
            .to_string();

        let cat_raw = &rec[40..72];
        let cat_len = cat_raw
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(cat_raw.len());
        let category = String::from_utf8_lossy(&cat_raw[..cat_len])
            .trim()
            .to_string();

        let price = u32le(rec, 92)?;
        let base_condition = f32::from_le_bytes(rec[96..100].try_into().unwrap_or([0; 4]));
        let modifier_value = f32::from_le_bytes(rec[192..196].try_into().unwrap_or([0; 4]));
        let secondary_value = f32::from_le_bytes(rec[196..200].try_into().unwrap_or([0; 4]));

        let mut torque_curve = Vec::new();
        if category.starts_with("Engine") && rec.len() >= 328 {
            for step in 0..14 {
                let off = 272 + step * 4;
                if off + 4 <= rec.len() {
                    let val = f32::from_le_bytes(rec[off..off + 4].try_into().unwrap_or([0; 4]));
                    torque_curve.push(val);
                }
            }
        }

        parts.push(PartRecord {
            id,
            name_string_id,
            car_model,
            category,
            price,
            base_condition,
            modifier_value,
            secondary_value,
            torque_curve,
        });
    }

    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_local_save_files() {
        let savedata_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/savedata");
        if !savedata_dir.exists() {
            return;
        }

        for save_name in ["0xfynx.sav", "XXDefXX.sav"] {
            let path = savedata_dir.join(save_name);
            if path.exists() {
                let data = std::fs::read(&path).unwrap();
                let save = parse_save(&data).unwrap();
                assert!(!save.sections.is_empty(), "sections empty in {save_name}");
                assert!(save.get_section("Missions").is_some());
                assert!(save.get_section("PlayerInfo").is_some());
                assert_eq!(save.mission_codes.len(), FACTORY_MISSION_COUNT);
                if save_name == "XXDefXX.sav" {
                    assert_eq!(save.cash, 11000); // Starting Evolution career credits
                    assert_eq!(save.owned_cars_count, 0);
                }
            }
        }
    }

    #[test]
    fn parse_local_car_catalog() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/FEData/Data/nfs5.car");
        if !path.exists() {
            return;
        }

        let data = std::fs::read(&path).unwrap();
        let catalog = parse_car_catalog(&data).unwrap();
        assert_eq!(catalog.len(), 109);
        assert!(catalog.iter().any(|c| c.model_name == "356_1"));
        assert!(catalog.iter().any(|c| c.sim_name == "356cabrio11"));
    }

    #[test]
    fn parse_local_track_catalog() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/FEData/Data/nfs5.trk");
        if !path.exists() {
            return;
        }

        let data = std::fs::read(&path).unwrap();
        let catalog = parse_track_catalog(&data).unwrap();
        assert_eq!(catalog.len(), TRACK_CATALOG_COUNT);
        assert!(catalog.iter().any(|t| t.internal_name == "coastal"));
        assert!(catalog.iter().all(|t| (300..=315).contains(&t.name_string)));
        let alps = catalog.iter().find(|t| t.internal_name == "alps").unwrap();
        assert_eq!((alps.photo_pack.as_str(), alps.unlock), ("AlpsSp", 3));
        for t in &catalog {
            let c = t.internal_name.starts_with("monaco") || t.internal_name == "skidpad";
            assert_eq!(t.circuit, c, "{}", t.internal_name);
        }
        let j = track_catalog_json(&catalog);
        assert!(j.starts_with("[{\"index\":0,\"internal_name\":\"coastal\""));
        assert_eq!(j.matches("\"index\"").count(), 15);
    }

    #[test]
    fn parse_local_factory_missions() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/FEData/Data/nfs5.fac");
        if !path.exists() {
            return;
        }

        let data = std::fs::read(&path).unwrap();
        let missions = parse_factory_missions(&data).unwrap();
        assert_eq!(missions.len(), FACTORY_MISSION_COUNT);
        assert_eq!(missions[0].code, "0M01");
        assert_eq!(missions[0].time_limit_sec, 32);
        assert_eq!(missions[0].track_id, 13); // skidpad
        assert_eq!(missions[1].code, "1m01");
    }

    #[test]
    fn parse_local_tournaments() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/FEData/Data/nfs5.trn");
        if !path.exists() {
            return;
        }

        let data = std::fs::read(&path).unwrap();
        let tournaments = parse_tournaments(&data).unwrap();
        assert_eq!(tournaments.len(), TOURNAMENT_COUNT);
        // Classic Tournament 1: 356 Challenge
        let t0 = &tournaments[0];
        assert_eq!(t0.title_string_id, 2018); // "356 Challenge"
        assert_eq!(t0.desc_string_id, 2036);
        assert_eq!(t0.num_stages, 2);
        assert_eq!(t0.entry_fee, 750);
        assert_eq!(t0.stages.len(), 2);
        assert_eq!(t0.stages[0].track_id, 14); // canyon
        assert_eq!(t0.stages[0].prizes[0], 4500); // 1st place prize
        assert_eq!(t0.stages[1].track_id, 8); // monaco1
        assert_eq!(t0.stages[1].prizes[0], 4500);
    }

    #[test]
    fn parse_local_parts_catalog() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/FEData/Data/nfs5.prt");
        if !path.exists() {
            return;
        }

        let data = std::fs::read(&path).unwrap();
        let parts = parse_parts_catalog(&data).unwrap();
        assert_eq!(parts.len(), PARTS_CATALOG_COUNT);
        let p0 = &parts[0];
        assert_eq!(p0.id, 13);
        assert_eq!(p0.car_model, "356");
        assert_eq!(p0.category, "Engine");
        assert_eq!(p0.price, 900);
        assert_eq!(p0.base_condition, 100.0);
    }
}

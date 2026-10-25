//! Car catalogue `FEData/Data/nfs5.car`: a flat array of 1648-byte records.
//!
//! Strings are NUL-padded ASCII. Offsets (inspected in the installed file, 109 records):
//! +0x02 display name (Latin-1), +0x34 short name, +0x4D long id (year + model), +0x7F model
//! (`.crp` base name), +0xB1 SIM name (`.sim`/`.spl` base name), +0xE3 showroom id, +0x115 family,
//! +0x147 logo pack, +0x179 engine sound bank (`sd1a`, ...). See `docs/formats/car-catalogue.md`.
//! The bank offset is confirmed by the original loader (`docs/formats/engine-sound-curves.md`).
use crate::Result;

pub const CAR_RECORD_SIZE: usize = 1648;
const DISPLAY_NAME_AT: usize = 0x02;
const SHORT_NAME_AT: usize = 0x34;
const MODEL_AT: usize = 0x7F;
const FAMILY_AT: usize = 0x115;
const SIM_NAME_AT: usize = 0xB1;
const BANK_NAME_AT: usize = 0x179;
const FIELD_LEN: usize = 32;
/// Gauge calibration `+0x2f0..+0x2fc` (4 f32): written by the front end as the race config
/// keys `.TACH0RPM`, `.TACH1000RPM`, `.SPEEDO0KM`, `.SPEEDO100KM` (`FE_WriteCarConfig`
/// 0x4eeda0 from object `+0x2f4` = record `+0x2f0`), read as `res+0x524..0x530` by
/// `R3DCar_SetAnimChannels` 0x438440. See `docs/re/cockpit.md`.
const GAUGES_AT: usize = 0x2f0;

#[derive(Debug, Clone, PartialEq)]
pub struct CarEntry {
    /// Record index in the file.
    pub index: usize,
    pub display_name: String,
    pub short_name: String,
    /// Model (`.crp`) base name, e.g. `356_1`, `F996`; match file stems case-insensitively.
    pub model: String,
    pub family: String,
    /// SIM name, e.g. `356Acabrio16`.
    pub sim_name: String,
    /// Engine sound bank archive base name, e.g. `sd1a`.
    pub bank_name: String,
    /// Needle calibration in turns: rev needle at 0 rpm, per 1000 rpm; speed needle at 0,
    /// per unit of `0.144·v` (`GAUGES_AT`).
    pub gauges: [f32; 4],
}

fn cstr(r: &[u8], at: usize, len: usize) -> String {
    let f = &r[at..at + len];
    let end = f.iter().position(|&b| b == 0).unwrap_or(len);
    f[..end].iter().map(|&b| b as char).collect()
}

/// Parses all records; the file size must be a multiple of 1648.
pub fn parse_car_catalogue(d: &[u8]) -> Result<Vec<CarEntry>> {
    if d.is_empty() || !d.len().is_multiple_of(CAR_RECORD_SIZE) {
        return Err(format!(
            "nfs5.car size {} is not a multiple of {CAR_RECORD_SIZE}",
            d.len()
        ));
    }
    Ok(d.as_chunks::<CAR_RECORD_SIZE>()
        .0
        .iter()
        .enumerate()
        .map(|(index, r)| CarEntry {
            index,
            display_name: cstr(r, DISPLAY_NAME_AT, SHORT_NAME_AT - DISPLAY_NAME_AT),
            short_name: cstr(r, SHORT_NAME_AT, FIELD_LEN),
            model: cstr(r, MODEL_AT, FIELD_LEN),
            family: cstr(r, FAMILY_AT, FIELD_LEN),
            sim_name: cstr(r, SIM_NAME_AT, FIELD_LEN),
            bank_name: cstr(r, BANK_NAME_AT, FIELD_LEN),
            gauges: std::array::from_fn(|i| {
                let o = GAUGES_AT + 4 * i;
                f32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]])
            }),
        })
        .collect())
}

/// First entry whose SIM name matches (ASCII case-insensitive). Some names appear twice
/// (e.g. `356BcoupeGS20` with banks `sd1a` and `sd1b`); the first record wins.
pub fn find_car<'a>(entries: &'a [CarEntry], sim_name: &str) -> Option<&'a CarEntry> {
    entries
        .iter()
        .find(|e| e.sim_name.eq_ignore_ascii_case(sim_name))
}

/// Records offered for play: they have a SIM name and both their model and SIM file stems are in
/// the given sets (ASCII case-insensitive). Catalogue order is kept; `index` stays the record index.
pub fn playable<M, S>(
    entries: &[CarEntry],
    installed_models: M,
    installed_sims: S,
) -> Vec<&CarEntry>
where
    M: IntoIterator,
    M::Item: AsRef<str>,
    S: IntoIterator,
    S::Item: AsRef<str>,
{
    let lower = |it: &mut dyn Iterator<Item = String>| -> std::collections::HashSet<String> {
        it.map(|s| s.to_ascii_lowercase()).collect()
    };
    let models = lower(&mut installed_models.into_iter().map(|s| s.as_ref().to_string()));
    let sims = lower(&mut installed_sims.into_iter().map(|s| s.as_ref().to_string()));
    entries
        .iter()
        .filter(|e| {
            !e.sim_name.is_empty()
                && models.contains(&e.model.to_ascii_lowercase())
                && sims.contains(&e.sim_name.to_ascii_lowercase())
        })
        .collect()
}

fn json_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// JSON array `[{index, display_name, model, sim_name, bank, gauges}, ...]` for the web front end.
pub fn catalogue_json(entries: &[CarEntry]) -> String {
    let mut out = String::from("[");
    for (i, e) in entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("{{\"index\":{},", e.index));
        for (k, v) in [
            ("display_name", &e.display_name),
            ("model", &e.model),
            ("sim_name", &e.sim_name),
            ("bank", &e.bank_name),
        ] {
            out.push_str(&format!("\"{k}\":"));
            json_str(v, &mut out);
            out.push(',');
        }
        let g: Vec<String> = e
            .gauges
            .iter()
            .map(|x| {
                if x.is_finite() {
                    format!("{x}")
                } else {
                    "0".into()
                }
            })
            .collect();
        out.push_str(&format!("\"gauges\":[{}]}}", g.join(",")));
    }
    out.push(']');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(sim: &str, bank: &str) -> Vec<u8> {
        let mut r = vec![0u8; CAR_RECORD_SIZE];
        r[DISPLAY_NAME_AT..DISPLAY_NAME_AT + 3].copy_from_slice(b"Car");
        r[SIM_NAME_AT..SIM_NAME_AT + sim.len()].copy_from_slice(sim.as_bytes());
        r[BANK_NAME_AT..BANK_NAME_AT + bank.len()].copy_from_slice(bank.as_bytes());
        r
    }

    #[test]
    fn parses_synthetic_records() {
        let mut d = record("abc12", "sd9");
        d.extend(record("xyz", "sd1a"));
        let e = parse_car_catalogue(&d).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].sim_name, "abc12");
        assert_eq!(e[0].bank_name, "sd9");
        assert_eq!(e[0].display_name, "Car");
        assert_eq!(find_car(&e, "XYZ").unwrap().bank_name, "sd1a");
        assert!(find_car(&e, "nope").is_none());
    }

    #[test]
    fn playable_filters_by_sim_and_installed_files() {
        let rec = |model: &str, sim: &str| {
            let mut r = record(sim, "sd1a");
            r[MODEL_AT..MODEL_AT + model.len()].copy_from_slice(model.as_bytes());
            r
        };
        let mut d = rec("F996", "996T");
        d.extend(rec("356_1", ""));
        d.extend(rec("Gone", "gone"));
        d.extend(rec("356_1", "NoSim"));
        d.extend(rec("356_1", "356x"));
        let e = parse_car_catalogue(&d).unwrap();
        let p = playable(&e, ["f996", "356_1", "gone"], ["996t", "356X", "nosim2"]);
        assert_eq!(p.iter().map(|c| c.index).collect::<Vec<_>>(), vec![0, 4]);
        assert_eq!(p[0].model, "F996");
    }

    #[test]
    fn json_escapes_and_keeps_fields() {
        let e = CarEntry {
            index: 7,
            display_name: "Coup\u{e9} \"x\"\\".into(),
            short_name: String::new(),
            model: "F996".into(),
            family: String::new(),
            sim_name: "996T".into(),
            bank_name: "sd7a".into(),
            gauges: [-0.07, 0.14, -0.01, 0.17],
        };
        assert_eq!(
            catalogue_json(&[e]),
            "[{\"index\":7,\"display_name\":\"Coup\u{e9} \\\"x\\\"\\\\\",\"model\":\"F996\",\"sim_name\":\"996T\",\"bank\":\"sd7a\",\"gauges\":[-0.07,0.14,-0.01,0.17]}]"
        );
        assert_eq!(catalogue_json(&[]), "[]");
    }

    #[test]
    fn rejects_bad_size() {
        assert!(parse_car_catalogue(&[0; 100]).is_err());
        assert!(parse_car_catalogue(&[]).is_err());
    }

    #[test]
    fn installed_catalogue_maps_banks() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/FEData/Data/nfs5.car");
        let Ok(d) = std::fs::read(&p) else {
            eprintln!("skip: nfs5.car not installed");
            return;
        };
        let e = parse_car_catalogue(&d).unwrap();
        assert_eq!(find_car(&e, "356Acabrio16").unwrap().bank_name, "sd1a");
        assert_eq!(find_car(&e, "356cabrio11").unwrap().bank_name, "sd1b");
        // Gauge calibration (docs/re/cockpit.md): 356A record 8 and 993 record 56.
        assert_eq!(e[8].gauges, [-0.07, 0.14, -0.01, 0.17]);
        assert_eq!(e[56].gauges, [-0.37, 0.09, -0.37, 0.07]);
    }

    fn stems(dir: &str, ext: &str) -> Option<Vec<String>> {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game")
            .join(dir);
        let rd = std::fs::read_dir(p).ok()?;
        Some(
            rd.flatten()
                .filter_map(|f| {
                    let n = f.file_name().to_string_lossy().to_string();
                    let (stem, e) = n.rsplit_once('.')?;
                    e.eq_ignore_ascii_case(ext).then(|| stem.to_string())
                })
                .collect(),
        )
    }

    #[test]
    fn installed_game_playable_cars() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/FEData/Data/nfs5.car");
        let (Ok(d), Some(models), Some(sims)) = (
            std::fs::read(&p),
            stems("GameData/CarModel", "crp"),
            stems("GameData/Simulation/CarData", "sim"),
        ) else {
            eprintln!("skip: game not installed");
            return;
        };
        let e = parse_car_catalogue(&d).unwrap();
        let p = playable(&e, &models, &sims);
        assert_eq!(p.len(), 92);
        assert_eq!(p[0].display_name, "'50 356 1100 Cabriolet");
        assert!(
            e.iter().any(|c| c.display_name.len() > FIELD_LEN),
            "long names are kept whole"
        );
        for n in ["Eden2", "EAS6"] {
            assert!(p.iter().all(|c| c.display_name != n));
        }
        let t = p
            .iter()
            .find(|c| c.display_name == "'00 911 Turbo (996)" && c.model == "F996")
            .unwrap();
        assert_eq!(
            (t.model.as_str(), t.sim_name.as_str(), t.bank_name.as_str()),
            ("F996", "996Tcoupe36", "sd7a")
        );
    }
}

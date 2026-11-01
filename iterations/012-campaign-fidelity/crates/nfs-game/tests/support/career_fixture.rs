//! Reader for the `replay-career-*.py` recordings: `size`, `trn` (table bytes) and op rows.
#![allow(dead_code)]

use std::collections::HashMap;

pub struct Fixture {
    pub table: Vec<u8>,
    pub header: Vec<String>,
    pub rows: Vec<HashMap<String, String>>,
}

pub fn parse(text: &str) -> Fixture {
    let mut table = Vec::new();
    let mut header: Vec<String> = Vec::new();
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "size" => table = vec![0u8; f[1].parse().unwrap()],
            "trn" => {
                let off = usize::from_str_radix(f[1], 16).unwrap();
                for (k, pair) in f[2].as_bytes().chunks(2).enumerate() {
                    table[off + k] = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
                }
            }
            _ if header.is_empty() => header = f.iter().map(|s| s.to_string()).collect(),
            _ => rows.push(header.iter().cloned().zip(f.iter().map(|s| s.to_string())).collect()),
        }
    }
    Fixture { table, header, rows }
}

/// FNV-1a 64 as the recorder hashes the save payload.
pub fn fnv64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    h
}

pub fn int(row: &HashMap<String, String>, k: &str) -> i32 {
    row[k].parse().unwrap_or_else(|_| panic!("{k} = {:?}", row[k]))
}

/// Recordings with `car` / `part` table lines (`replay-career-garage.py`, `-parts.py`).
pub struct TableFixture {
    pub cars: Vec<u8>,
    pub parts: Vec<u8>,
    pub present: std::collections::HashSet<Vec<u8>>,
    pub rows: Vec<HashMap<String, String>>,
}

pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|k| u8::from_str_radix(&s[2 * k..2 * k + 2], 16).unwrap()).collect()
}

pub fn parse_tables(text: &str) -> TableFixture {
    let mut fx = TableFixture { cars: Vec::new(), parts: Vec::new(), present: Default::default(), rows: Vec::new() };
    let mut header: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "car" => fx.cars.extend(hex(f[2])),
            "part" => fx.parts.extend(hex(f[2])),
            "present" => {
                fx.present.insert(f[1].as_bytes().to_vec());
            }
            _ if header.is_empty() => header = f.iter().map(|s| s.to_string()).collect(),
            _ => fx.rows.push(header.iter().cloned().zip(f.iter().map(|s| s.to_string())).collect()),
        }
    }
    fx
}

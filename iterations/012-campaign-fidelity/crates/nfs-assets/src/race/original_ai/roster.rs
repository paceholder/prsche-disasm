//! Opponent drivers of a Quick Race: `FEData/Data/nfs5.per` ("CPersonalities", 0x2c-byte
//! records loaded by `0x4e2f30`) and the pick `FUN_004e3160(count, difficulty)`
//! (`docs/re/ai.md` "Difficulty and the opponent roster").

/// One driver record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Driver {
    /// Record id (i16 at +0).
    pub id: i16,
    /// Name string id (i32 at +4).
    pub name_string: i32,
    /// Difficulty level of the roster (i16 at +8): 0 beginner, 1 advanced, 2 expert.
    pub level: i16,
    /// Skill values for `res+0x154..+0x174`: the four base values (record +0xc, +0x14, +0x1c,
    /// +0x24) then their four ranges (+0x10, +0x18, +0x20, +0x28), keys 0x23f..0x246.
    pub skill_154: [f32; 8],
}

const RECORD: usize = 0x2c;

/// Parses `nfs5.per`; a trailing partial record is ignored.
pub fn parse_personalities(bytes: &[u8]) -> Vec<Driver> {
    let f = |r: &[u8], o: usize| f32::from_le_bytes(r[o..o + 4].try_into().unwrap());
    bytes
        .chunks_exact(RECORD)
        .map(|r| Driver {
            id: i16::from_le_bytes([r[0], r[1]]),
            name_string: i32::from_le_bytes(r[4..8].try_into().unwrap()),
            level: i16::from_le_bytes([r[8], r[9]]),
            skill_154: [
                f(r, 0x0c),
                f(r, 0x14),
                f(r, 0x1c),
                f(r, 0x24),
                f(r, 0x10),
                f(r, 0x18),
                f(r, 0x20),
                f(r, 0x28),
            ],
        })
        .collect()
}

/// `FUN_004e3160`: `count` distinct drivers drawn at random from those of difficulty `level`
/// (`None`: all). `rand` is the C `rand()` stand-in. Fewer come back when the roster is smaller
/// (the original would divide by zero; its rosters hold 7, 7 and 21 drivers).
pub fn pick_opponents(
    drivers: &[Driver],
    count: usize,
    level: Option<i16>,
    mut rand: impl FnMut() -> u32,
) -> Vec<Driver> {
    let mut pool: Vec<Driver> = drivers
        .iter()
        .copied()
        .filter(|d| level.is_none_or(|l| d.level == l))
        .collect();
    let mut out = Vec::new();
    while out.len() < count && !pool.is_empty() {
        let i = rand() as usize % pool.len();
        out.push(pool.remove(i));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: i16, level: i16, base: f32) -> Vec<u8> {
        let mut r = vec![0u8; RECORD];
        r[0..2].copy_from_slice(&id.to_le_bytes());
        r[4..8].copy_from_slice(&(18000 + id as i32).to_le_bytes());
        r[8..10].copy_from_slice(&level.to_le_bytes());
        for (k, o) in [0x0c, 0x14, 0x1c, 0x24].into_iter().enumerate() {
            r[o..o + 4].copy_from_slice(&(base + k as f32 * 0.01).to_le_bytes());
            r[o + 4..o + 8].copy_from_slice(&0.05f32.to_le_bytes());
        }
        r
    }

    #[test]
    fn records_parse_into_base_values_then_ranges() {
        let bytes: Vec<u8> = [record(1, 0, 0.6), record(9, 1, 0.77)].concat();
        let d = parse_personalities(&bytes);
        assert_eq!(d.len(), 2);
        assert_eq!((d[1].id, d[1].level, d[1].name_string), (9, 1, 18009));
        assert_eq!(d[1].skill_154[0], 0.77);
        assert!((d[1].skill_154[3] - 0.80).abs() < 1e-6);
        assert_eq!(d[1].skill_154[4..], [0.05; 4]);
    }

    #[test]
    fn pick_draws_distinct_drivers_of_the_level() {
        let bytes: Vec<u8> = (0..9)
            .flat_map(|i| record(i, (i / 3) as i16, 0.5 + i as f32 * 0.01))
            .collect();
        let d = parse_personalities(&bytes);
        let mut n = 0u32;
        let picked = pick_opponents(&d, 3, Some(1), || {
            n += 7;
            n
        });
        assert_eq!(picked.len(), 3);
        assert!(picked.iter().all(|p| p.level == 1));
        let mut ids: Vec<_> = picked.iter().map(|p| p.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 3);
        // More than the roster holds: the roster.
        assert_eq!(pick_opponents(&d, 9, Some(2), || 1).len(), 3);
    }
}

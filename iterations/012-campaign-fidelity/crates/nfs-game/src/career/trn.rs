//! The `nfs5.trn` tournament table, kept as the original keeps it: one mutable blob of
//! 0xd40-byte records edited in place (`g_TournRecords` 0x691e58, count 0x691e64), saved whole
//! in the `Tournament` save section.
//!
//! Offsets are the ones the original consumers use (workspace `docs/re/career.md`,
//! `docs/formats/trn.md`). Reads outside the blob return 0 and writes outside it are dropped;
//! the original would touch neighbouring heap memory there (only reachable with places
//! outside 1..8 or an empty table).

/// Record size in bytes.
pub const RECORD_SIZE: usize = 0xd40;
/// Race entry size in bytes; races start at record +0x3a0.
pub const RACE_SIZE: usize = 0xa4;
/// Highest race count kept after loading (`Tourn_BuildEraLists` 0x502ba0 clamps +0x39c).
pub const MAX_RACES: i16 = 10;

/// Era list ids (string ids of the era names, `festrings.h`).
pub const ERA_CLASSIC: i32 = 0x209;
pub const ERA_GOLDEN: i32 = 0x20a;
pub const ERA_MODERN: i32 = 0x20b;
/// Club event lists ("CLASSIC/GOLDEN/MODERN CLUB EVENTS").
pub const CLUB_CLASSIC: i32 = 0x20c;
pub const CLUB_GOLDEN: i32 = 0x20d;
pub const CLUB_MODERN: i32 = 0x20e;
/// The six lists in the order of `Tourn_BuildEraLists` 0x502ba0.
pub const LISTS: [i32; 6] = [ERA_CLASSIC, ERA_GOLDEN, ERA_MODERN, CLUB_CLASSIC, CLUB_GOLDEN, CLUB_MODERN];

/// Record field offsets.
pub mod off {
    pub const ID: usize = 0x000; // i16
    pub const LIST: usize = 0x004; // i32 era / club list id
    pub const YEAR: usize = 0x008; // i16 era start year (0 for clubs)
    pub const TRACK_COUNT: usize = 0x00a; // i16
    pub const TRACKS: usize = 0x00c; // i16[]
    pub const SHORT_NAME: usize = 0x020; // i32 string id
    pub const TITLE: usize = 0x024; // i32 string id
    pub const DESCRIPTION: usize = 0x028; // i32 string id
    pub const RESTRICTION: usize = 0x02c; // i32 string id
    pub const ALLOWED_CARS: usize = 0x030; // i16[count]
    pub const ALLOWED_COUNT: usize = 0x0da; // i16
    pub const ENTRY_FEE: usize = 0x0dc; // i32
    pub const OPPONENTS: usize = 0x0e0; // i16 index into {0,2,4,8} (0x5db438)
    pub const KIND: usize = 0x0e4; // i16, 2 = end-of-era bonus race
    pub const SORT_MODE: usize = 0x0e8; // u8, standings sort
    pub const OVERALL_PRIZE: usize = 0x0ec; // i32[8] by final rank 1..8
    pub const PRIZE_CAR: usize = 0x10c; // i16 car id of a type-2 record
    pub const FIXED_ROSTER: usize = 0x380; // u8
    pub const ROSTER: usize = 0x382; // i16[7]
    pub const RACE_COUNT: usize = 0x39c; // i16
    pub const RACES: usize = 0x3a0; // race[RACE_COUNT]
    pub const COMPLETED: usize = 0xd3c; // i16 times completed
}

/// Race entry field offsets (relative to the race).
pub mod race_off {
    pub const TRACK: usize = 0x00; // i16 track id (nfs5.trk +0x00)
    pub const LAPS: usize = 0x02; // i16, 0 means 1
    pub const REVERSE: usize = 0x04; // u8
    pub const FLAG_05: usize = 0x05; // u8
    pub const PRIZE: usize = 0x08; // i32[8] by place
    pub const BONUS_BASE: usize = 0x28; // i32
    pub const STRENGTH: usize = 0x2c; // f32
    pub const DONE: usize = 0x30; // u8
    pub const POINTS: usize = 0x32; // i16[8] per slot
    pub const TIME: usize = 0x44; // i32[8] per slot
    pub const BONUS: usize = 0x64; // i32[8] per slot
    pub const WINNINGS: usize = 0x84; // i32[8] per slot
}

/// The tournament table blob.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TournamentTable {
    blob: Vec<u8>,
}

impl TournamentTable {
    /// Takes the raw `nfs5.trn` bytes (or a saved blob) and applies the race-count clamp of
    /// `Tourn_BuildEraLists` 0x502ba0. A trailing partial record is kept in the blob but not
    /// counted, as `size / 0xd40` does.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut t = TournamentTable { blob: bytes.to_vec() };
        for i in 0..t.len() {
            if t.i16(i, off::RACE_COUNT) > MAX_RACES {
                t.set_i16(i, off::RACE_COUNT, MAX_RACES);
            }
        }
        t
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.blob
    }

    /// Number of records (`0x691e64` = size / 0xd40).
    pub fn len(&self) -> usize {
        self.blob.len() / RECORD_SIZE
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // -- raw access --------------------------------------------------------
    fn abs(rec: usize, o: usize) -> usize {
        rec * RECORD_SIZE + o
    }

    pub fn raw_i16(&self, at: usize) -> i16 {
        self.blob.get(at..at + 2).map_or(0, |b| i16::from_le_bytes([b[0], b[1]]))
    }

    pub fn raw_i32(&self, at: usize) -> i32 {
        self.blob.get(at..at + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn raw_u8(&self, at: usize) -> u8 {
        self.blob.get(at).copied().unwrap_or(0)
    }

    pub fn raw_set_i16(&mut self, at: usize, v: i16) {
        if let Some(b) = self.blob.get_mut(at..at + 2) {
            b.copy_from_slice(&v.to_le_bytes());
        }
    }

    pub fn raw_set_i32(&mut self, at: usize, v: i32) {
        if let Some(b) = self.blob.get_mut(at..at + 4) {
            b.copy_from_slice(&v.to_le_bytes());
        }
    }

    pub fn raw_set_u8(&mut self, at: usize, v: u8) {
        if let Some(b) = self.blob.get_mut(at) {
            *b = v;
        }
    }

    pub fn i16(&self, rec: usize, o: usize) -> i16 {
        self.raw_i16(Self::abs(rec, o))
    }
    pub fn i32(&self, rec: usize, o: usize) -> i32 {
        self.raw_i32(Self::abs(rec, o))
    }
    pub fn u8(&self, rec: usize, o: usize) -> u8 {
        self.raw_u8(Self::abs(rec, o))
    }
    pub fn set_i16(&mut self, rec: usize, o: usize, v: i16) {
        self.raw_set_i16(Self::abs(rec, o), v)
    }
    pub fn set_i32(&mut self, rec: usize, o: usize, v: i32) {
        self.raw_set_i32(Self::abs(rec, o), v)
    }
    pub fn set_u8(&mut self, rec: usize, o: usize, v: u8) {
        self.raw_set_u8(Self::abs(rec, o), v)
    }

    /// Offset of race `race` of record `rec` within the record.
    pub fn race(race: usize) -> usize {
        off::RACES + race * RACE_SIZE
    }

    // -- typed fields ------------------------------------------------------
    pub fn id(&self, rec: usize) -> i16 {
        self.i16(rec, off::ID)
    }
    pub fn list(&self, rec: usize) -> i32 {
        self.i32(rec, off::LIST)
    }
    pub fn kind(&self, rec: usize) -> i16 {
        self.i16(rec, off::KIND)
    }
    pub fn race_count(&self, rec: usize) -> i16 {
        self.i16(rec, off::RACE_COUNT)
    }
    pub fn completed(&self, rec: usize) -> i16 {
        self.i16(rec, off::COMPLETED)
    }
    pub fn entry_fee(&self, rec: usize) -> i32 {
        self.i32(rec, off::ENTRY_FEE)
    }
    pub fn sort_mode(&self, rec: usize) -> u8 {
        self.u8(rec, off::SORT_MODE)
    }
    pub fn overall_prize(&self, rec: usize, rank: i32) -> i32 {
        // Tourn_OverallPrize 0x505770: record + 0xe8 + 4 * rank.
        self.raw_i32((Self::abs(rec, 0xe8) as i64 + 4 * rank as i64).max(0) as usize)
    }
    pub fn allowed_cars(&self, rec: usize) -> Vec<i16> {
        let n = self.i16(rec, off::ALLOWED_COUNT).max(0) as usize;
        (0..n).map(|k| self.i16(rec, off::ALLOWED_CARS + 2 * k)).collect()
    }
    pub fn race_done(&self, rec: usize, race: usize) -> bool {
        self.u8(rec, Self::race(race) + race_off::DONE) != 0
    }
    pub fn race_track(&self, rec: usize, race: usize) -> i16 {
        self.i16(rec, Self::race(race) + race_off::TRACK)
    }

    /// `Tourn_FindById` 0x503f20: index of the first record with this id, else record 0.
    pub fn find(&self, id: i32) -> usize {
        (0..self.len()).find(|&i| self.id(i) as i32 == id).unwrap_or(0)
    }

    /// Races of a record whose done flag is clear (`Tourn_RacesLeftById` 0x504300 body).
    pub fn races_left(&self, rec: usize) -> i32 {
        let n = self.race_count(rec).max(0) as usize;
        (0..n).filter(|&r| !self.race_done(rec, r)).count() as i32
    }

    /// `Tourn_ResetResults` 0x503e40 body: done flag and the four per-slot result arrays of
    /// every race of the record.
    pub fn clear_results(&mut self, rec: usize) {
        let n = self.race_count(rec).max(0) as usize;
        for r in 0..n {
            let b = Self::race(r);
            self.set_u8(rec, b + race_off::DONE, 0);
            for s in 0..8 {
                self.set_i16(rec, b + race_off::POINTS + 2 * s, 0);
                self.set_i32(rec, b + race_off::TIME + 4 * s, 0);
                self.set_i32(rec, b + race_off::BONUS + 4 * s, 0);
                self.set_i32(rec, b + race_off::WINNINGS + 4 * s, 0);
            }
        }
    }

    /// Id lists of `Tourn_BuildEraLists` 0x502ba0: ids of the records of each list in table
    /// order, end-of-era (type 2) records skipped.
    pub fn list_ids(&self, list: i32) -> Vec<i32> {
        (0..self.len())
            .filter(|&i| self.kind(i) != 2 && self.list(i) == list && LISTS.contains(&list))
            .map(|i| self.id(i) as i32)
            .collect()
    }

    /// Readable view of one record for front-end screens.
    pub fn record(&self, rec: usize) -> TournamentInfo {
        let n = self.race_count(rec).max(0) as usize;
        TournamentInfo {
            index: rec,
            id: self.id(rec),
            list: self.list(rec),
            year: self.i16(rec, off::YEAR),
            tracks: (0..self.i16(rec, off::TRACK_COUNT).clamp(0, 10) as usize)
                .map(|k| self.i16(rec, off::TRACKS + 2 * k))
                .collect(),
            short_name: self.i32(rec, off::SHORT_NAME),
            title: self.i32(rec, off::TITLE),
            description: self.i32(rec, off::DESCRIPTION),
            restriction: self.i32(rec, off::RESTRICTION),
            allowed_cars: self.allowed_cars(rec),
            entry_fee: self.entry_fee(rec),
            opponents_index: self.i16(rec, off::OPPONENTS),
            kind: self.kind(rec),
            sort_mode: self.sort_mode(rec),
            overall_prize: std::array::from_fn(|k| self.i32(rec, off::OVERALL_PRIZE + 4 * k)),
            prize_car: self.i16(rec, off::PRIZE_CAR),
            fixed_roster: self.u8(rec, off::FIXED_ROSTER) != 0,
            roster: std::array::from_fn(|k| self.i16(rec, off::ROSTER + 2 * k)),
            completed: self.completed(rec),
            races: (0..n)
                .map(|r| {
                    let b = Self::race(r);
                    RaceInfo {
                        track: self.i16(rec, b + race_off::TRACK),
                        laps: self.i16(rec, b + race_off::LAPS),
                        reverse: self.u8(rec, b + race_off::REVERSE) != 0,
                        flag_05: self.u8(rec, b + race_off::FLAG_05),
                        prize: std::array::from_fn(|k| self.i32(rec, b + race_off::PRIZE + 4 * k)),
                        bonus_base: self.i32(rec, b + race_off::BONUS_BASE),
                        strength: f32::from_bits(self.i32(rec, b + race_off::STRENGTH) as u32),
                        done: self.race_done(rec, r),
                        points: std::array::from_fn(|s| self.i16(rec, b + race_off::POINTS + 2 * s)),
                        time: std::array::from_fn(|s| self.i32(rec, b + race_off::TIME + 4 * s)),
                        bonus: std::array::from_fn(|s| self.i32(rec, b + race_off::BONUS + 4 * s)),
                        winnings: std::array::from_fn(|s| self.i32(rec, b + race_off::WINNINGS + 4 * s)),
                    }
                })
                .collect(),
        }
    }
}

/// Decoded record (copy) for screens and tests.
#[derive(Clone, Debug, PartialEq)]
pub struct TournamentInfo {
    pub index: usize,
    pub id: i16,
    pub list: i32,
    pub year: i16,
    /// **Hypothesis:** tracks shown for the era (consumer not found).
    pub tracks: Vec<i16>,
    pub short_name: i32,
    pub title: i32,
    pub description: i32,
    pub restriction: i32,
    /// Car ids allowed to enter (`Tourn_CarAllowed` 0x504a70).
    pub allowed_cars: Vec<i16>,
    pub entry_fee: i32,
    pub opponents_index: i16,
    pub kind: i16,
    pub sort_mode: u8,
    pub overall_prize: [i32; 8],
    pub prize_car: i16,
    pub fixed_roster: bool,
    pub roster: [i16; 7],
    pub completed: i16,
    pub races: Vec<RaceInfo>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RaceInfo {
    pub track: i16,
    pub laps: i16,
    pub reverse: bool,
    pub flag_05: u8,
    pub prize: [i32; 8],
    pub bonus_base: i32,
    /// **Hypothesis:** AI strength (0.45..1.0).
    pub strength: f32,
    pub done: bool,
    pub points: [i16; 8],
    pub time: [i32; 8],
    pub bonus: [i32; 8],
    pub winnings: [i32; 8],
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(n: usize) -> TournamentTable {
        let mut b = vec![0u8; n * RECORD_SIZE];
        for i in 0..n {
            b[i * RECORD_SIZE..i * RECORD_SIZE + 2].copy_from_slice(&(i as i16 + 1).to_le_bytes());
        }
        TournamentTable::from_bytes(&b)
    }

    #[test]
    fn race_count_is_clamped_to_ten_on_load() {
        let mut b = vec![0u8; 2 * RECORD_SIZE];
        b[off::RACE_COUNT..off::RACE_COUNT + 2].copy_from_slice(&12i16.to_le_bytes());
        b[RECORD_SIZE + off::RACE_COUNT..RECORD_SIZE + off::RACE_COUNT + 2].copy_from_slice(&9i16.to_le_bytes());
        let t = TournamentTable::from_bytes(&b);
        assert_eq!((t.race_count(0), t.race_count(1)), (10, 9));
    }

    #[test]
    fn find_falls_back_to_record_zero() {
        let t = table(3);
        assert_eq!((t.find(2), t.find(3), t.find(99)), (1, 2, 0));
    }

    #[test]
    fn out_of_range_access_is_inert() {
        let mut t = table(1);
        t.set_i32(5, 0, 7);
        assert_eq!(t.i32(5, 0), 0);
        assert_eq!(t.len(), 1);
    }
}

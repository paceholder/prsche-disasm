//! The `Tournament` save section (`Tourn_SaveSection` 0x502ae0 / `Tourn_LoadSection`
//! 0x5029f0). Payload, little-endian: year, next race, current tournament index, era,
//! running race, running tournament, Evolution-complete flag, blob size (i32 each), then
//! the whole table blob. The section name `Tournament\0` precedes the payload in the
//! `.sav` file (`Save_WriteProfile` 0x4deda0); no size field.

use super::{trn::TournamentTable, Career};

/// Section name in the profile list (`Tourn_RegisterSave` 0x5027e0).
pub const SECTION_NAME: &str = "Tournament";

#[derive(Debug, PartialEq, Eq)]
pub enum SaveError {
    /// Payload shorter than the header or than the blob size it declares.
    Truncated { need: usize, have: usize },
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Truncated { need, have } => write!(f, "Tournament section truncated: need {need} bytes, have {have}"),
        }
    }
}

impl std::error::Error for SaveError {}

const HEADER: usize = 8 * 4;

impl Career {
    /// `Tourn_SaveSection` 0x502ae0: the payload bytes.
    pub fn save_tournament_section(&self) -> Vec<u8> {
        let blob = self.table.as_bytes();
        let mut out = Vec::with_capacity(HEADER + blob.len());
        for v in [
            self.year,
            self.next_race,
            self.cur_tourn,
            self.era,
            self.running_race,
            self.running_tourn,
            self.evolution_complete,
            blob.len() as i32,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(blob);
        out
    }

    /// `Tourn_LoadSection` 0x5029f0: restores the tournament globals and the table from a
    /// payload (then `Tourn_BuildEraLists`: race-count clamp and lists). Returns the bytes
    /// consumed. Cash and the era-changed flag are not part of the section and keep their
    /// values.
    pub fn load_tournament_section(&mut self, payload: &[u8]) -> Result<usize, SaveError> {
        if payload.len() < HEADER {
            return Err(SaveError::Truncated { need: HEADER, have: payload.len() });
        }
        let w = |k: usize| i32::from_le_bytes(payload[4 * k..4 * k + 4].try_into().unwrap());
        let size = w(7) as u32 as usize;
        let need = HEADER + size;
        if payload.len() < need {
            return Err(SaveError::Truncated { need, have: payload.len() });
        }
        self.year = w(0);
        self.next_race = w(1);
        self.cur_tourn = w(2);
        self.era = w(3);
        self.running_race = w(4);
        self.running_tourn = w(5);
        self.evolution_complete = w(6);
        self.table = TournamentTable::from_bytes(&payload[HEADER..need]);
        self.build_era_lists();
        Ok(need)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::career::trn::RECORD_SIZE;

    #[test]
    fn section_round_trips() {
        let mut blob = vec![0u8; 3 * RECORD_SIZE];
        blob[0] = 4;
        let mut c = Career::new(&blob);
        c.year = 1973;
        c.running_tourn = 2;
        c.table.set_i16(1, 0xd3c, 5);
        let bytes = c.save_tournament_section();
        assert_eq!(bytes.len(), 32 + 3 * RECORD_SIZE);
        assert_eq!(&bytes[..4], &1973i32.to_le_bytes());
        let mut d = Career::new(&[]);
        assert_eq!(d.load_tournament_section(&bytes), Ok(bytes.len()));
        assert_eq!(d.save_tournament_section(), bytes);
        assert_eq!(d.table.completed(1), 5);
        assert_eq!(d.load_tournament_section(&bytes[..40]), Err(SaveError::Truncated { need: bytes.len(), have: 40 }));
    }
}

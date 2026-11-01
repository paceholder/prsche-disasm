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
    /// A car's part id is not in the parts table (`OwnedCar_Read` fails).
    MissingPart,
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Truncated { need, have } => write!(f, "save section truncated: need {need} bytes, have {have}"),
            SaveError::MissingPart => write!(f, "save section: part id not in the parts table"),
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

// ---------------------------------------------------------------------------------------
// PlayerInfo, UsedCarMarket, PartsMarket.

use super::catalogue::{PartRecord, PartsCatalogue, PART_RECORD_SIZE};
use super::garage::{sort_parts, OwnedCar, OWNED_CAR_SIZE};
use super::used::MarketSlot;

/// Section names of the profile (`[0x65b638]` save list).
pub const PLAYER_SECTION: &str = "PlayerInfo";
pub const USED_MARKET_SECTION: &str = "UsedCarMarket";
pub const PARTS_SECTION: &str = "PartsMarket";

/// Bytes of the car image the profile stores (all but the two vector pointers).
const CAR_IMAGE_SAVED: usize = 0x758;

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], SaveError> {
        let need = self.at + n;
        if need > self.b.len() {
            return Err(SaveError::Truncated { need, have: self.b.len() });
        }
        let s = &self.b[self.at..need];
        self.at = need;
        Ok(s)
    }
    fn i32(&mut self) -> Result<i32, SaveError> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u8(&mut self) -> Result<u8, SaveError> {
        Ok(self.take(1)?[0])
    }
    /// A NUL-terminated string (the original copies it with `sprintf(dst, src)`).
    fn cstr(&mut self) -> Result<Vec<u8>, SaveError> {
        let rest = &self.b[self.at.min(self.b.len())..];
        let n = rest.iter().position(|&c| c == 0).ok_or(SaveError::Truncated { need: self.b.len() + 1, have: self.b.len() })?;
        let s = rest[..n].to_vec();
        self.at += n + 1;
        Ok(s)
    }
}

/// Errors of the car part lists in a profile.
impl SaveError {
    fn missing_part() -> Self {
        SaveError::MissingPart
    }
}

impl OwnedCar {
    /// `OwnedCar_Write` 0x4ee8d0: the first 0x758 image bytes, the stock and installed counts,
    /// then (id, condition bits) of every stock part followed by every installed part.
    pub fn write_save(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.image[..CAR_IMAGE_SAVED]);
        out.extend_from_slice(&(self.stock.len() as i32).to_le_bytes());
        out.extend_from_slice(&(self.installed.len() as i32).to_le_bytes());
        for p in self.stock.iter().chain(self.installed.iter()) {
            out.extend_from_slice(&(p.id() as i32).to_le_bytes());
            out.extend_from_slice(&p.condition().to_bits().to_le_bytes());
        }
    }

    /// `OwnedCar_Read` 0x4ee6a0 into a default-constructed car (`0x4ee5c0`, not in the car
    /// registry): the image, then the part lists rebuilt from the parts table with the saved
    /// conditions (a missing id fails the load), the installed list sorted by `Part_Less`.
    fn read_save(r: &mut Reader, parts: &PartsCatalogue) -> Result<OwnedCar, SaveError> {
        let mut image = vec![0u8; OWNED_CAR_SIZE];
        image[..CAR_IMAGE_SAVED].copy_from_slice(r.take(CAR_IMAGE_SAVED)?);
        let n_stock = r.i32()?;
        let n_inst = r.i32()?;
        let mut car = OwnedCar { image, installed: Vec::new(), stock: Vec::new(), handle: 0, registered: false };
        for k in 0..n_stock.wrapping_add(n_inst).max(0) {
            let id = r.i32()?;
            let cond = f32::from_bits(r.i32()? as u32);
            let mut p: PartRecord = parts.find(id).cloned().ok_or_else(SaveError::missing_part)?;
            p.set_condition(cond);
            if k < n_stock {
                car.stock.push(p);
            } else {
                car.installed.push(p);
            }
        }
        sort_parts(&mut car.installed);
        Ok(car)
    }
}

/// The `PlayerInfo` globals besides the garage and the cash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerInfo {
    /// `[0x5db748]`: driver figure of career races (8 on a new profile).
    pub driver: i32,
    /// `[0x691dc8]`: player name.
    pub name: Vec<u8>,
    /// `[0x691de8]`: second name field (13 bytes before the flag 0x691df5).
    pub name2: Vec<u8>,
    /// `[0x691dd5]`: the careerfirst screen was shown.
    pub career_started: u8,
    /// `[0x691d94]`.
    pub flag_d94: u8,
    /// `[0x691df5]`.
    pub flag_df5: u8,
    /// `[0x691d98]`: eight ints (meaning not read).
    pub ints: [i32; 8],
}

impl Default for PlayerInfo {
    /// `Player_NewProfile` 0x501340 (names are set by the profile screens).
    fn default() -> Self {
        PlayerInfo { driver: 8, name: Vec::new(), name2: Vec::new(), career_started: 0, flag_d94: 0, flag_df5: 0, ints: [0; 8] }
    }
}

impl Career {
    /// `Player_SaveSection` 0x5018b0.
    pub fn save_player_section(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(self.garage.cars.len() as i32).to_le_bytes());
        for c in &self.garage.cars {
            c.write_save(&mut out);
        }
        let p = &self.player;
        out.extend_from_slice(&self.cash.to_le_bytes());
        out.extend_from_slice(&p.driver.to_le_bytes());
        for s in [&p.name, &p.name2] {
            out.extend_from_slice(s);
            out.push(0);
        }
        out.extend_from_slice(&[p.career_started, p.flag_d94, p.flag_df5]);
        for v in p.ints {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    /// `Player_LoadSection` 0x501610 (the garage is replaced; availability for `game_type`).
    /// Returns the bytes consumed.
    pub fn load_player_section(&mut self, b: &[u8], parts: &PartsCatalogue, game_type: i32) -> Result<usize, SaveError> {
        let mut r = Reader { b, at: 0 };
        self.garage.cars.clear();
        let n = r.i32()?;
        for _ in 0..n.max(0) {
            let mut c = OwnedCar::read_save(&mut r, parts)?;
            c.handle = self.next_handle();
            self.garage.cars.push(c);
        }
        self.update_available_cars(game_type);
        self.cash = r.i32()?;
        self.player.driver = r.i32()?;
        self.player.name = r.cstr()?;
        self.player.name2 = r.cstr()?;
        self.player.career_started = r.u8()?;
        self.player.flag_d94 = r.u8()?;
        self.player.flag_df5 = r.u8()?;
        for k in 0..8 {
            self.player.ints[k] = r.i32()?;
        }
        Ok(r.at)
    }

    /// `UsedMarket_SaveSection` 0x4f7a10: count and year, then the market cars (+0x6a0 = 0;
    /// the player's listed cars are not saved here).
    pub fn save_used_market_section(&self) -> Vec<u8> {
        let mut body = Vec::new();
        let mut n = 0i32;
        for s in &self.used_market.slots {
            if let MarketSlot::Market(c) = s {
                if c.owned_flag() == 0 {
                    c.write_save(&mut body);
                    n += 1;
                }
            }
        }
        let mut out = Vec::with_capacity(8 + body.len());
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&self.used_market.year.to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    /// `UsedMarket_LoadSection` 0x4f7750: the market is replaced by the saved market cars
    /// (listed garage cars come back with [`Career::relist_cars`]).
    pub fn load_used_market_section(&mut self, b: &[u8], parts: &PartsCatalogue) -> Result<usize, SaveError> {
        let mut r = Reader { b, at: 0 };
        self.used_market.slots.clear();
        self.used_market.year = 1950;
        let n = r.i32()?;
        self.used_market.year = r.i32()?;
        for _ in 0..n.max(0) {
            let mut c = OwnedCar::read_save(&mut r, parts)?;
            c.handle = self.next_handle();
            self.used_market.slots.push(MarketSlot::Market(c));
        }
        Ok(r.at)
    }

    /// `Parts_SaveSection` 0x517a30: count, then the spare part records (0x168 bytes each).
    pub fn save_parts_section(&self) -> Vec<u8> {
        let mut out = (self.spare_parts.len() as i32).to_le_bytes().to_vec();
        for p in &self.spare_parts {
            out.extend_from_slice(p.bytes());
        }
        out
    }

    /// `Parts_LoadSection` 0x517810.
    pub fn load_parts_section(&mut self, b: &[u8]) -> Result<usize, SaveError> {
        let mut r = Reader { b, at: 0 };
        self.spare_parts.clear();
        let n = r.i32()?;
        for _ in 0..n.max(0) {
            self.spare_parts.push(PartRecord(r.take(PART_RECORD_SIZE)?.to_vec()));
        }
        Ok(r.at)
    }

    /// `Garage_RelistCars` 0x501a70 (end of `Save_ReadProfile` 0x4de9b0): garage cars with
    /// the listed flag +0x691 go back on the market at their asking price.
    pub fn relist_cars(&mut self, game_type: i32) {
        for g in 0..self.garage.cars.len() {
            if self.garage.cars[g].listed_flag() != 0 {
                let asking = self.garage.cars[g].asking_price();
                self.list_car_for_sale(g, asking);
            }
        }
        self.update_available_cars(game_type);
    }
}

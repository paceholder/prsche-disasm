//! Evolution progression: tournament selection and launch, race results, standings, money,
//! and the year/era state machine. Each method ports the named original function
//! (workspace `docs/re/career.md` "Race results and money", "Progression state machine").

use super::trn::{off, race_off, TournamentTable, CLUB_CLASSIC, CLUB_GOLDEN, CLUB_MODERN, ERA_CLASSIC, ERA_GOLDEN, ERA_MODERN};
use super::{strings, Career, CareerEvent, YEAR_TABLE};

/// One car's result as `Race_ApplyResults` 0x4edf00 reads it from the race-results block
/// 0x5e77a0 (stride 0x258): place +0, race time +4, flag byte at −0x14.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarResult {
    /// Finishing place, 1 = first.
    pub place: i32,
    /// Race time as the race stores it.
    pub time: i32,
    /// The byte at −0x14 of the car's result: when set, no podium bonus.
    /// **Hypothesis:** the car had contact (bonus key `TOURNAMENTRACESAFEBONUSES`).
    pub bonus_blocked: bool,
}

/// Game type of the race whose results are applied (`[0x690ad4]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaceKind {
    /// Game type 4: an Evolution tournament race.
    Tournament,
    /// Game type 10: the end-of-era bonus race.
    EraBonus,
}

/// Input of [`Career::apply_race_results`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaceResults {
    pub kind: RaceKind,
    /// One entry per car slot (slot 0 = the player).
    pub cars: Vec<CarResult>,
    /// `[0x657e34]`: the race was left without a result.
    pub aborted: bool,
    /// Debug `[0x657e20]`: complete the current tournament with full marks.
    pub debug_complete_tournament: bool,
    /// Debug `[0x657e21]`: complete the current era.
    pub debug_complete_era: bool,
}

impl RaceResults {
    pub fn tournament(cars: Vec<CarResult>) -> Self {
        RaceResults { kind: RaceKind::Tournament, cars, aborted: false, debug_complete_tournament: false, debug_complete_era: false }
    }
}

/// What `Race_ApplyResults` changed outside the career state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ApplyOutcome {
    /// Add to the player car's odometer (`car+0x698`).
    pub odometer: i32,
    /// Add to the player car's win counter (`car+0x69c`). The original adds one for every
    /// car of the race whose place is 1, i.e. one per race with a winner, whoever won.
    pub wins: i32,
    /// Game type 10 won (player place 1): the prize car goes into the garage
    /// (`Garage_AddPrizeCar` 0x502270 with the `PrizeCars.Car` accessor car).
    pub prize_car_won: bool,
}

/// Why the front end does not start a tournament race (`FE_LaunchRace` 0x4d5730).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchRefusal {
    /// The car is not in the tournament's allowed list (string 813).
    CarNotAllowed,
    /// Another tournament is partly played: ask (string 858); on yes the front end clears it
    /// (`Tourn_ResetAbandoned` 0x504bd0) and launches again (0x4d2b80).
    ConfirmAbandon,
    /// Cash below the entry fee (string 857).
    NoFunds,
}

impl LaunchRefusal {
    pub fn string_id(self) -> i32 {
        match self {
            LaunchRefusal::CarNotAllowed => strings::FAIL_RESTRICTIONS,
            LaunchRefusal::ConfirmAbandon => strings::PROCEED_WITH_TOURNAMENT,
            LaunchRefusal::NoFunds => strings::NO_FUNDS_FOR_TOURNY,
        }
    }
}

fn is_club(list: i32) -> bool {
    matches!(list, CLUB_CLASSIC | CLUB_GOLDEN | CLUB_MODERN)
}

/// `__ftol` of an x87 product (precision 53 bits, truncation).
fn ftol(x: f64) -> i32 {
    x as i64 as i32
}

impl Career {
    fn rec(&self, idx: i32) -> usize {
        // The original indexes the table with the global unchecked; a negative index reads
        // past the end here (inert: reads 0, writes dropped).
        if idx < 0 { self.table.len() } else { idx as usize }
    }

    // -- simple queries ---------------------------------------------------------------
    /// `Career_GetYear` 0x5032c0.
    pub fn year(&self) -> i32 {
        self.year
    }

    /// `Career_GetPrevEra` 0x5032e0 (an unknown era returns it unchanged here; the original
    /// returns a stale register).
    pub fn previous_era(&self) -> i32 {
        match self.era {
            ERA_CLASSIC | ERA_GOLDEN => ERA_CLASSIC,
            ERA_MODERN => ERA_GOLDEN,
            e => e,
        }
    }

    /// `Era_StartYear` 0x5049e0.
    pub fn era_start_year(era: i32) -> Option<i32> {
        match era {
            ERA_CLASSIC => Some(1950),
            ERA_GOLDEN => Some(1970),
            ERA_MODERN => Some(1989),
            _ => None,
        }
    }

    /// `Era_EndYear` 0x502990 (and `Era_EndYearB` 0x504a10).
    pub fn era_end_year(era: i32) -> Option<i32> {
        match era {
            ERA_CLASSIC => Some(1970),
            ERA_GOLDEN => Some(1989),
            ERA_MODERN => Some(2000),
            _ => None,
        }
    }

    /// `Career_SetYear` 0x504170 (debug/accessor): the era follows the year.
    pub fn set_year(&mut self, year: i32) {
        self.year = year;
        self.era = if year < 1989 {
            ERA_CLASSIC + (year > 1969) as i32
        } else {
            ERA_MODERN
        };
    }

    /// `Tourn_CurrentId` 0x503310: id of the current record.
    pub fn current_id(&self) -> i32 {
        self.table.id(self.rec(self.cur_tourn)) as i32
    }

    /// `Era_IsComplete` 0x503ed0: every type-2-excluded record of the list completed.
    pub fn era_is_complete(&self, era: i32) -> bool {
        (0..self.table.len()).all(|i| !(self.table.list(i) == era && self.table.kind(i) != 2 && self.table.completed(i) == 0))
    }

    /// `Career_HighestCompletedEra` 0x504800: the last completed era of Classic, Golden,
    /// Modern (checked in that order, a later complete era wins). `None` when none is
    /// complete: the original then returns whatever ECX held.
    pub fn highest_completed_era(&self) -> Option<i32> {
        let mut r = None;
        if self.era_is_complete(ERA_CLASSIC) {
            r = Some(ERA_CLASSIC);
        }
        if self.era_is_complete(ERA_GOLDEN) {
            r = Some(ERA_GOLDEN);
        }
        if self.era_is_complete(ERA_MODERN) {
            r = Some(ERA_MODERN);
        }
        r
    }

    /// `Era_FindType2Record` 0x504990: index of the end-of-era bonus record of a list, else 0.
    pub fn era_bonus_record(&self, era: i32) -> usize {
        (0..self.table.len()).find(|&i| self.table.list(i) == era && self.table.kind(i) == 2).unwrap_or(0)
    }

    /// `Tourn_CarAllowed` 0x504a70: the car id is in the record's allowed list.
    pub fn car_allowed(&self, id: i32, car: i32) -> bool {
        self.table.allowed_cars(self.table.find(id)).iter().any(|&c| c as i32 == car)
    }

    /// `0x504b60`: some record other than the current one has played some but not all races.
    pub fn other_tournament_partly_played(&self) -> bool {
        (0..self.table.len()).any(|i| {
            let n = self.table.race_count(i) as i32;
            let left = self.table.races_left(i);
            n > 0 && left > 0 && left < n && i as i32 != self.cur_tourn
        })
    }

    /// `Tourn_ResetAbandoned` 0x504bd0: clears the results of every partly played record
    /// other than the current one (the clear goes to the first record with that id).
    pub fn reset_abandoned(&mut self) {
        for i in 0..self.table.len() {
            let n = self.table.race_count(i) as i32;
            let left = self.table.races_left(i);
            if n > 0 && left > 0 && left < n && i as i32 != self.cur_tourn {
                let r = self.table.find(self.table.id(i) as i32);
                self.table.clear_results(r);
            }
        }
    }

    /// `Tourn_SelectList` 0x505010: list 0 = the current era, 1..3 = the club lists. The
    /// current record becomes the first uncompleted one of the list, else the first one of
    /// the list, else −1. (The front end's race-index vector it also fills is not kept.)
    pub fn select_list(&mut self, list: i32) {
        let list = match list {
            0 => self.era,
            1 => CLUB_CLASSIC,
            2 => CLUB_GOLDEN,
            3 => CLUB_MODERN,
            l => l,
        };
        let mut first = -1;
        for i in 0..self.table.len() {
            if self.table.list(i) == list && self.table.kind(i) != 2 {
                if self.table.completed(i) == 0 {
                    self.cur_tourn = i as i32;
                    return;
                }
                if first == -1 {
                    first = i as i32;
                }
            }
        }
        self.cur_tourn = first;
    }

    /// `Player_AddCash` 0x5012e0: the change is clamped to ±999 999 999.
    pub fn add_cash(&mut self, delta: i32) {
        let d = delta.min(999_999_999).max(-999_999_999);
        self.cash = self.cash.wrapping_add(d);
    }

    /// `Tourn_SetRunning` 0x503940: the next race of the current record is the race run.
    pub fn set_running(&mut self) {
        self.running_race = self.next_race;
        self.running_tourn = self.cur_tourn;
    }

    /// `Tourn_ClearRunning` 0x503a50.
    pub fn clear_running(&mut self) {
        self.next_race = 0;
        self.running_race = -1;
        self.running_tourn = -1;
    }

    /// Entry fee step of `FE_LaunchRace` 0x4d5730 for game type 4 (after its car and
    /// abandon checks): before the first race of the current tournament the cash must cover
    /// the fee (`Tourn_NumRaces` 0x503370 == `Tourn_RacesLeftById` 0x504300, `Tourn_EntryFee`
    /// 0x503430), which is then paid; the race becomes the running one.
    pub fn pay_entry_and_start(&mut self) -> Result<(), LaunchRefusal> {
        let rec = self.table.find(self.current_id());
        if self.table.race_count(rec) as i32 == self.table.races_left(rec) {
            let fee = self.table.entry_fee(rec);
            if self.cash < fee {
                return Err(LaunchRefusal::NoFunds);
            }
            self.add_cash(fee.wrapping_neg());
        }
        self.set_running();
        Ok(())
    }

    /// The Evolution checks of `FE_LaunchRace` 0x4d5730 in order: car eligibility
    /// (`Tourn_CarAllowed`, string 813), another tournament partly played (string 858;
    /// `abandon_confirmed` = the player said yes: `Tourn_ResetAbandoned`, 0x4d2b80), then
    /// the entry fee. Read from the decompile; the pieces are recorded.
    pub fn launch_tournament_race(&mut self, car_id: i32, abandon_confirmed: bool) -> Result<(), LaunchRefusal> {
        if !self.car_allowed(self.current_id(), car_id) {
            return Err(LaunchRefusal::CarNotAllowed);
        }
        if self.other_tournament_partly_played() {
            if !abandon_confirmed {
                return Err(LaunchRefusal::ConfirmAbandon);
            }
            self.reset_abandoned();
        }
        self.pay_entry_and_start()
    }

    /// Unfinished races of the running tournament (`Tourn_RacesLeft` 0x5056d0).
    pub fn running_races_left(&self) -> i32 {
        self.table.races_left(self.rec(self.running_tourn))
    }

    fn running_race_field(&self, o: usize) -> i32 {
        let rec = self.rec(self.running_tourn);
        let at = rec.wrapping_mul(super::trn::RECORD_SIZE) as i64
            + (TournamentTable::race(0) + o) as i64
            + self.running_race as i64 * super::trn::RACE_SIZE as i64;
        if at < 0 { 0 } else { self.table.raw_i32(at as usize) }
    }

    /// `Tourn_PlayerRaceWinnings` 0x505740: slot 0 winnings of the running race.
    pub fn player_race_winnings(&self) -> i32 {
        self.running_race_field(race_off::WINNINGS)
    }

    /// `Tourn_PlayerRaceBonus` 0x5057a0: slot 0 bonus of the running race.
    pub fn player_race_bonus(&self) -> i32 {
        self.running_race_field(race_off::BONUS)
    }

    /// `Tourn_RaceTrackId` 0x5057d0: track id of the running race.
    pub fn running_track(&self) -> i32 {
        self.running_race_field(race_off::TRACK) as i16 as i32
    }

    /// `Tourn_PlayerRank` 0x505710: standing of slot 0 in the running tournament.
    pub fn player_rank(&self) -> i32 {
        let id = self.table.id(self.rec(self.running_tourn)) as i32;
        self.standing(id, 0)
    }

    // -- results ------------------------------------------------------------------------
    /// `Tourn_RecordRaceResult` 0x503960: slot result of the running race: points 9 − place,
    /// time, winnings = race prize of the place; with `bonus_ok` a bonus of 100 / 50 / 25 %
    /// of the race bonus base for places 1 / 2 / 3 (0 otherwise); the race is marked done.
    /// The record is the first one with the running record's id.
    pub fn record_race_result(&mut self, slot: i32, time: i32, place: i32, bonus_ok: bool) {
        let rec = self.table.find(self.table.id(self.rec(self.running_tourn)) as i32);
        let base = (rec * super::trn::RECORD_SIZE + TournamentTable::race(0)) as i64
            + self.running_race as i64 * super::trn::RACE_SIZE as i64;
        let at = |o: i64| -> usize { (base + o).max(0) as usize };
        let t = &mut self.table;
        t.raw_set_i16(at(race_off::POINTS as i64 + 2 * slot as i64), 9i16.wrapping_sub(place as i16));
        t.raw_set_i32(at(race_off::TIME as i64 + 4 * slot as i64), time);
        let prize = t.raw_i32(at(race_off::PRIZE as i64 - 4 + 4 * place as i64));
        t.raw_set_i32(at(race_off::WINNINGS as i64 + 4 * slot as i64), prize);
        if bonus_ok {
            let b = t.raw_i32(at(race_off::BONUS_BASE as i64));
            let bonus = match place {
                1 => b,
                2 => ftol(b as f64 * 0.5f32 as f64),  // [0x5b23f0]
                3 => ftol(b as f64 * 0.25f32 as f64), // [0x5b2468]
                _ => 0,
            };
            t.raw_set_i32(at(race_off::BONUS as i64 + 4 * slot as i64), bonus);
        }
        t.raw_set_u8(at(race_off::DONE as i64), 1);
    }

    /// `Tourn_Standing` 0x503660: rank (1..8) of `slot` in tournament `id` from the sums
    /// over finished races of points, times and money (bonus + winnings), sorted by
    /// `0x502880` with the record's sort mode. With no race finished: `slot`, or 8 for slot 0.
    pub fn standing(&self, id: i32, slot: i32) -> i32 {
        let rec = self.table.find(id);
        let t = &self.table;
        let n = t.race_count(rec).max(0) as usize;
        if t.races_left(rec) == t.race_count(rec) as i32 {
            return if slot == 0 { 8 } else { slot };
        }
        let mut e = [[0i32; 4]; 8];
        for (s, entry) in e.iter_mut().enumerate() {
            let mut pts = 0i32;
            let mut time = 0i32;
            let mut money = 0i32;
            for r in (0..n).filter(|&r| t.race_done(rec, r)) {
                let b = TournamentTable::race(r);
                pts = pts.wrapping_add(t.i16(rec, b + race_off::POINTS + 2 * s) as i32);
                time = time.wrapping_add(t.i32(rec, b + race_off::TIME + 4 * s));
                money = money
                    .wrapping_add(t.i32(rec, b + race_off::WINNINGS + 4 * s))
                    .wrapping_add(t.i32(rec, b + race_off::BONUS + 4 * s));
            }
            *entry = [pts, time, money, s as i32];
        }
        sort_standings(&mut e, t.sort_mode(rec) != 0);
        e.iter().position(|x| x[3] == slot).map_or(9, |p| p as i32 + 1)
    }

    /// `Tourn_CheatComplete` 0x504370: every race of the record done with slot 0 points 8,
    /// winnings = first prize, bonus = full bonus base.
    pub fn debug_complete_tournament(&mut self, id: i32) {
        let rec = self.table.find(id);
        for r in 0..self.table.race_count(rec).max(0) as usize {
            let b = TournamentTable::race(r);
            let t = &mut self.table;
            t.set_i16(rec, b + race_off::POINTS, 8);
            let p = t.i32(rec, b + race_off::PRIZE);
            t.set_i32(rec, b + race_off::WINNINGS, p);
            let bb = t.i32(rec, b + race_off::BONUS_BASE);
            t.set_i32(rec, b + race_off::BONUS, bb);
            t.set_u8(rec, b + race_off::DONE, 1);
        }
    }

    /// `Era_CheatComplete` 0x504450: `Tourn_CheatComplete` + `Tourn_OnTournamentDone` for
    /// every id of the era's list (lists of Classic, Golden, Modern only).
    pub fn debug_complete_era(&mut self, era: i32) {
        if !matches!(era, ERA_CLASSIC | ERA_GOLDEN | ERA_MODERN) {
            return;
        }
        let ids = self.list_ids(era).to_vec();
        for id in ids {
            self.debug_complete_tournament(id);
            self.on_tournament_done(id);
        }
    }

    /// `Race_ApplyResults` 0x4edf00 for game types 4 and 10.
    pub fn apply_race_results(&mut self, r: &RaceResults) -> ApplyOutcome {
        let mut out = ApplyOutcome::default();
        if r.aborted {
            if r.kind == RaceKind::Tournament {
                self.clear_running();
                let id = self.current_id();
                self.on_tournament_done(id);
            }
            return out;
        }
        let track = match r.kind {
            RaceKind::Tournament => self.running_track(),
            // Race_GetSetup 0x4fc0b0 case 10: the bonus record's race (not ported here).
            RaceKind::EraBonus => -1,
        };
        self.events.push(CareerEvent::TrackRecords { track });
        if r.kind == RaceKind::Tournament {
            if r.debug_complete_era {
                self.debug_complete_era(self.era);
            } else if r.debug_complete_tournament {
                let id = self.current_id();
                self.debug_complete_tournament(id);
            } else {
                for (slot, c) in r.cars.iter().enumerate() {
                    self.record_race_result(slot as i32, c.time, c.place, !c.bonus_blocked);
                    if c.place == 1 {
                        out.wins += 1;
                    }
                }
            }
            out.odometer = 3000;
            self.add_cash(self.player_race_winnings());
            self.add_cash(self.player_race_bonus());
            if self.running_races_left() == 0 {
                let rank = self.player_rank();
                let prize = self.table.overall_prize(self.rec(self.running_tourn), rank);
                self.add_cash(prize);
            }
            self.clear_running();
            let id = self.current_id();
            self.on_tournament_done(id);
        }
        if r.kind == RaceKind::EraBonus && r.cars.first().is_some_and(|c| c.place == 1) {
            out.prize_car_won = true;
        }
        self.events.push(CareerEvent::DebugPrint);
        out
    }

    /// The "pick the next unfinished race of the current record" tail of
    /// `Tourn_OnTournamentDone`: 0x691e7c = first race without the done flag (0 if none).
    fn pick_next_race(&mut self) {
        self.next_race = 0;
        let rec = self.rec(self.cur_tourn);
        let n = self.table.race_count(rec) as i32;
        if n < 1 {
            return;
        }
        let mut i = 0;
        while self.table.race_done(rec, i as usize) {
            i += 1;
            if n <= i {
                return;
            }
        }
        self.next_race = i;
    }

    /// `Tourn_OnTournamentDone` 0x503ad0: called after every race result and on abort.
    pub fn on_tournament_done(&mut self, id: i32) {
        let rec = self.table.find(id);
        self.era_changed = false;
        // Every record with this id has all its races done.
        let finished = (0..self.table.len()).filter(|&i| self.table.id(i) as i32 == id).all(|i| self.table.races_left(i) == 0);
        if !finished {
            self.pick_next_race();
            self.events.push(CareerEvent::RebuildCarLists);
            return;
        }
        if is_club(self.table.list(rec)) {
            let c = self.table.completed(rec);
            self.table.set_i16(rec, off::COMPLETED, c.wrapping_add(1));
            self.table.clear_results(rec);
            self.events.push(CareerEvent::RebuildCarLists);
            return;
        }
        if self.standing(id, 0) <= 3 {
            let c = self.table.completed(rec).wrapping_add(1);
            self.table.set_i16(rec, off::COMPLETED, c);
            if c == 1 {
                if let Some(k) = YEAR_TABLE[..YEAR_TABLE.len() - 1].iter().position(|&y| y == self.year) {
                    self.year = YEAR_TABLE[k + 1];
                }
            }
            if self.era_is_complete(self.era) {
                if self.era == ERA_MODERN {
                    self.evolution_complete = 1;
                }
                self.era = match self.era {
                    ERA_CLASSIC => ERA_GOLDEN,
                    ERA_GOLDEN | ERA_MODERN => ERA_MODERN,
                    _ => id,
                };
                self.era_changed = true;
            }
            // First uncompleted record of the (new) era, else the last completed one, else 0.
            let mut last = 0;
            let mut found = None;
            for i in 0..self.table.len() {
                if self.table.list(i) == self.era && self.table.kind(i) != 2 {
                    if self.table.completed(i) == 0 {
                        found = Some(i as i32);
                        break;
                    }
                    last = i as i32;
                }
            }
            self.cur_tourn = found.unwrap_or(last);
            self.events.push(CareerEvent::RaceListRefresh);
        }
        self.table.clear_results(self.table.find(id));
        self.events.push(CareerEvent::RebuildCarLists);
    }
}

/// `0x502880`: bubble sort of the {points, time, money, slot} entries, 8 passes over the 7
/// neighbour pairs. Mode 0: points descending, then money descending, then time ascending.
/// Mode 1: time ascending, then money ascending.
pub fn sort_standings(e: &mut [[i32; 4]; 8], by_time: bool) {
    for _ in 0..8 {
        for k in 0..7 {
            let (a, b) = (e[k], e[k + 1]);
            let swap = if !by_time {
                a[0] < b[0] || (a[0] == b[0] && (a[2] < b[2] || (a[2] == b[2] && b[1] < a[1])))
            } else {
                b[1] < a[1] || (b[1] == a[1] && b[2] < a[2])
            };
            if swap {
                e.swap(k, k + 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::career::trn::RECORD_SIZE;

    /// Two Classic records (ids 1, 2), one race each, record 0 allows car 7 only, fee 500.
    fn career() -> Career {
        let mut b = vec![0u8; 2 * RECORD_SIZE];
        for (i, id) in [1i16, 2].iter().enumerate() {
            let r = i * RECORD_SIZE;
            b[r..r + 2].copy_from_slice(&id.to_le_bytes());
            b[r + 4..r + 8].copy_from_slice(&ERA_CLASSIC.to_le_bytes());
            b[r + off::RACE_COUNT..r + off::RACE_COUNT + 2].copy_from_slice(&2i16.to_le_bytes());
        }
        b[off::ALLOWED_COUNT..off::ALLOWED_COUNT + 2].copy_from_slice(&1i16.to_le_bytes());
        b[off::ALLOWED_CARS..off::ALLOWED_CARS + 2].copy_from_slice(&7i16.to_le_bytes());
        b[off::ENTRY_FEE..off::ENTRY_FEE + 4].copy_from_slice(&500i32.to_le_bytes());
        Career::new(&b)
    }

    #[test]
    fn launch_checks_car_then_abandon_then_fee() {
        let mut c = career();
        assert_eq!(c.launch_tournament_race(8, false), Err(LaunchRefusal::CarNotAllowed));
        // Record 1 half played: asks first, clears it on yes, then pays the fee.
        c.table.set_u8(1, TournamentTable::race(0) + race_off::DONE, 1);
        assert_eq!(c.launch_tournament_race(7, false), Err(LaunchRefusal::ConfirmAbandon));
        assert_eq!(c.launch_tournament_race(7, true), Ok(()));
        assert!(!c.table.race_done(1, 0));
        assert_eq!((c.cash, c.running_tourn, c.running_race), (10_500, 0, 0));
        c.cash = 100;
        c.running_tourn = -1;
        assert_eq!(c.launch_tournament_race(7, true), Err(LaunchRefusal::NoFunds));
        assert_eq!(LaunchRefusal::NoFunds.string_id(), 857);
    }

    #[test]
    fn standings_sort_ties_like_the_original() {
        // Mode 0: points desc, money desc, time asc.
        let mut e = [[0, 0, 0, 0]; 8];
        e[0] = [10, 500, 100, 0];
        e[1] = [10, 400, 100, 1];
        e[2] = [10, 400, 200, 2];
        for (k, x) in e.iter_mut().enumerate().skip(3) {
            x[3] = k as i32;
        }
        sort_standings(&mut e, false);
        assert_eq!(e[..3].iter().map(|x| x[3]).collect::<Vec<_>>(), [2, 1, 0]);
        // Mode 1: time asc, then money asc; empty slots (time 0) rank first.
        sort_standings(&mut e, true);
        assert_eq!(e[7][3], 0);
        assert_eq!(e[0][1], 0);
    }

    #[test]
    fn bonus_race_win_reports_the_prize_car() {
        let mut c = career();
        let r = RaceResults { kind: RaceKind::EraBonus, ..RaceResults::tournament(vec![CarResult { place: 1, time: 5, bonus_blocked: false }]) };
        assert!(c.apply_race_results(&r).prize_car_won);
        let r = RaceResults { kind: RaceKind::EraBonus, ..RaceResults::tournament(vec![CarResult { place: 2, time: 5, bonus_blocked: false }]) };
        assert!(!c.apply_race_results(&r).prize_car_won);
    }
}

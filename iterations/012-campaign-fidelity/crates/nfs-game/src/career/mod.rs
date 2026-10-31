//! The original Evolution career of `nfs5.exe`, as pure logic (no screens).
//!
//! Reconstructed from the original code (workspace `docs/re/career.md`); every method names
//! the function it ports. VAs refer to `nfs5.exe` SHA-256 `c318af39…4906`
//! (`docs/binary-identity.md`). The prototype modules `crate::tournament`, `crate::economy`
//! and `crate::parts` (iteration 010) are not the original and are not used here.
//!
//! - [`trn`]: the `nfs5.trn` tournament table (blob edited in place).
//! - [`save`]: the `Tournament` save section.
//! - [`Career`]: the career globals and (in `progression`) the year/era state machine,
//!   tournaments, points and money.

pub mod save;
pub mod trn;

pub use trn::{RaceInfo, TournamentInfo, TournamentTable};

/// Year steps of the career (`g_CareerYearTable` 0x5b8670, int[17]).
pub const YEAR_TABLE: [i32; 17] =
    [1950, 1956, 1960, 1965, 1969, 1970, 1973, 1975, 1978, 1982, 1988, 1989, 1993, 1996, 1997, 1999, 2000];

/// Cash of a new profile (`Player_NewProfile` 0x501340).
pub const NEW_PROFILE_CASH: i32 = 11_000;
/// Initial `g_CareerYear` (.data value of 0x5db764; `Tourn_NewCareer` does not set it).
pub const INITIAL_YEAR: i32 = 1950;

/// String ids the career returns to the screens (`FEData/Locale/festrings.h`).
pub mod strings {
    /// IDS_NOMORETHANEIGHTY: the garage holds 80 cars.
    pub const NO_MORE_THAN_EIGHTY: i32 = 61;
    /// IDS_DIALOG_NOTENOUGHCASH.
    pub const NOT_ENOUGH_CASH: i32 = 802;
    /// IDS_FAILRESTRICTIONS: car not allowed in the tournament.
    pub const FAIL_RESTRICTIONS: i32 = 813;
    /// IDS_NOFUNDSFORTOURNY: cash below the entry fee.
    pub const NO_FUNDS_FOR_TOURNY: i32 = 857;
    /// IDS_PROCEEDWITHTOURNAMENT: another tournament is partly played.
    pub const PROCEED_WITH_TOURNAMENT: i32 = 858;
}

/// Side effects the original performs outside the career state (front-end lists, track
/// records, debug print). The career logs them; the caller drains and acts on them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CareerEvent {
    /// Track records update of the race track (`0x4fb100`, or `0x4fafc0` on a linear track).
    TrackRecords { track: i32 },
    /// `Career_RebuildCarLists` 0x4f3e40: dealer/used/owned car lists for the new year.
    RebuildCarLists,
    /// `0x5041b0`: the front end's race-index list of the current tournament.
    RaceListRefresh,
    /// `0x504a40`: "Current year is now %d / era %s" debug print.
    DebugPrint,
}

/// The career globals of the original and the tournament table.
#[derive(Clone, Debug, PartialEq)]
pub struct Career {
    pub table: TournamentTable,
    /// `g_CareerYear` 0x5db764.
    pub year: i32,
    /// `g_CareerEra` 0x691e50 (0x209 / 0x20a / 0x20b).
    pub era: i32,
    /// `g_EvolutionComplete` 0x691e54 (saved as an i32).
    pub evolution_complete: i32,
    /// `g_CurTournIdx` 0x691e6c: record index selected on the tournament screen.
    pub cur_tourn: i32,
    /// 0x691e7c: next race index of the current tournament.
    pub next_race: i32,
    /// 0x5db768: race index of the race being run (−1 = none).
    pub running_race: i32,
    /// 0x5db76c: record index of the tournament being run (−1 = none).
    pub running_tourn: i32,
    /// `g_EraChanged` 0x691e81: set when the last finish moved the era on (not saved).
    pub era_changed: bool,
    /// `g_Cash` 0x691d90 (part of the `PlayerInfo` section in the original).
    pub cash: i32,
    /// Side effects for the caller (see [`CareerEvent`]).
    pub events: Vec<CareerEvent>,
    era_lists: [Vec<i32>; 6],
}

impl Career {
    /// A new career: `Player_NewProfile` 0x501340 (cash 11 000) + `Tourn_NewCareer` 0x502ff0
    /// (era Classic, first tournament, pristine table) on the given `nfs5.trn` bytes.
    pub fn new(trn: &[u8]) -> Self {
        let table = TournamentTable::from_bytes(trn);
        let mut c = Career {
            table,
            year: INITIAL_YEAR,
            era: trn::ERA_CLASSIC,
            evolution_complete: 0,
            cur_tourn: 0,
            next_race: 0,
            running_race: -1,
            running_tourn: -1,
            era_changed: false,
            cash: NEW_PROFILE_CASH,
            events: Vec::new(),
            era_lists: Default::default(),
        };
        c.build_era_lists();
        c
    }

    /// `Tourn_BuildEraLists` 0x502ba0 (list part): ids per era/club list, type-2 records
    /// skipped. The race-count clamp is applied by [`TournamentTable::from_bytes`]. The debug
    /// auto-completion `[0x657e24]` and the FE accessor refresh are not run.
    pub(crate) fn build_era_lists(&mut self) {
        self.era_lists = trn::LISTS.map(|l| self.table.list_ids(l));
    }

    /// Ids of a list (`Era_IdList` 0x503340); empty for unknown list ids.
    pub fn list_ids(&self, list: i32) -> &[i32] {
        trn::LISTS.iter().position(|&l| l == list).map_or(&[], |k| &self.era_lists[k])
    }

    /// Takes the logged side effects.
    pub fn drain_events(&mut self) -> Vec<CareerEvent> {
        std::mem::take(&mut self.events)
    }
}

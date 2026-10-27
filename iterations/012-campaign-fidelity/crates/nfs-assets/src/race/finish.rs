//! Race progress, finish and race end of the original (`docs/re/race-finish.md` in the
//! workspace root).
//!
//! Source: the instructions of
//! - `Car_TrackProgressTick` `0x4169b0` (car queue `+0xe18`, prio 0x19, even steps) with
//!   `Race_ProgressTick` `0x416310` (lap and finish bookkeeping, race order table
//!   `0x608b38`) and `Car_ProgressLapCount` `0x417440`; the lap fraction of a node is
//!   `0x481b70` (`TrackNodes::progress_terms`);
//! - `Race_OrderClear` `0x416140` (even steps, before the car queue);
//! - `Race_EndCheck` `0x417010` and `Race_Places` `0x415fc0` (odd steps, after the queue);
//! - `Race_FinalResults` `0x416ac0` (`+0xcd0..+0xcfc`).
//!
//! Single player only: the network (`[0x6573e8] == 3`), knockout (`[0x6573ec] == 3`),
//! DERBY (`[0x657400]`) and catch-up (`0x415fc0` tail) branches, the damage and wear copies
//! of `0x416ac0` and the `+0xb54/+0xb58` checkpoint index of `0x417010` are not ported.
//! Verified against `scripts/research/replay-race-finish.py` (test
//! `tests/original_race_finish.rs`).

// Negated comparisons keep the x87 branches on unordered operands.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

/// Race states `+0xb50`.
pub const RACING: i32 = 0;
/// Abandoned (`0x416ac0(1)`, leaving the race).
pub const ABANDONED: i32 = 1;
pub const FINISHED: i32 = 2;
/// Out of the race (knockout, wreck).
pub const OUT: i32 = 3;

/// The race start: 7 s of 64 Hz steps before the green light (`0x1c0`).
pub const START_STEPS: i32 = 0x1c0;
/// `0x606850` after the player finished: the replay track cameras (`0x14`).
pub const CAMERA_TRACK: i32 = 0x14;

/// Race fields of one car (offsets of the original car struct).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RaceCar {
    /// `+0x520`: car index (player index for the humans).
    pub index_520: i32,
    /// `+0x52c`: bit 0 racer, bit 2 human.
    pub flags_52c: u32,
    /// `+0x08` (short): track node of the locator.
    pub node_08: i32,
    /// `+0xd60`: forward speed (m/s).
    pub speed_d60: f32,
    /// `+0xd00`: distance driven, `|d60| * 0.0003125` per tick.
    pub distance_d00: f32,
    /// `+0xad4[lap]`: top speed per lap.
    pub top_ad4: [f32; 8],
    /// `+0xaf4` / `+0xaf8`: last / current progress (laps + lap fraction).
    pub progress_af4: f32,
    pub progress_af8: f32,
    /// `+0xb00`: start-line fraction, `+0xb04`: finish-line fraction, `+0xb08`: behind
    /// the start (point-to-point: start − 0.005) (`0x410730`).
    pub start_b00: f32,
    pub finish_b04: f32,
    pub behind_b08: f32,
    /// `+0xb10`: nodes driven, `+0xb14`: step of the last node change (finish step once
    /// finished), `+0xb18`: last node, `+0xb20`: laps counted, `+0xb24`: lap start step,
    /// then the race time in steps after the green light once finished.
    pub nodes_b10: i32,
    pub step_b14: i32,
    pub node_b18: i32,
    pub laps_b20: i32,
    pub time_b24: i32,
    /// `+0xb28[lap]`: lap times in steps.
    pub lap_b28: [i32; 8],
    /// `+0xb48`: place (`0x415fc0`).
    pub place_b48: i32,
    /// `+0xb50`: race state.
    pub state_b50: i32,
    /// `+0x530`: laps, `+0x534`: laps driven backwards across the line.
    pub laps_530: i32,
    pub back_534: i32,
    /// `0x416ac0` results: `+0xcd0` state, `+0xcd4` place, `+0xcd8` time, `+0xcfc` best lap,
    /// `+0xcdc[lap]` lap times.
    pub result_cd0: i32,
    pub place_cd4: i32,
    pub time_cd8: i32,
    pub best_cfc: i32,
    pub laps_cdc: [i32; 8],
}

impl RaceCar {
    /// A racer at the start (`0x410730`): `start` is the lap fraction of the start node,
    /// `finish` that of the finish trigger's node (point-to-point); a circuit has 0, 1, −1.
    pub fn new(index: i32, human: bool, linear: bool, start: f32, finish: f32) -> Self {
        let (b00, b04, b08) = if linear {
            (start, finish, (start as f64 - 0.005) as f32)
        } else {
            (0.0, 1.0, -1.0)
        };
        Self {
            index_520: index,
            flags_52c: if human { 5 } else { 1 },
            time_b24: START_STEPS,
            start_b00: b00,
            finish_b04: b04,
            behind_b08: b08,
            ..Default::default()
        }
    }

    pub fn human(&self) -> bool {
        self.flags_52c & 4 != 0
    }
}

/// One entry of the race order table `0x608b38` (16 bytes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrderEntry {
    pub id: i32,
    pub progress: f32,
    pub step: i32,
    pub human: u32,
}

impl Default for OrderEntry {
    /// `Race_OrderClear` `0x416140`.
    fn default() -> Self {
        Self {
            id: -1,
            progress: -100.0,
            step: 0,
            human: 0,
        }
    }
}

/// Race globals.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RaceGlobals {
    /// `g_SchedStep [0x606ac4]`.
    pub step_606ac4: i32,
    /// `[0x6573f0]` laps (1 point-to-point).
    pub laps_6573f0: i32,
    /// `[0x657494]` point-to-point track.
    pub linear_657494: bool,
    /// `[0x657430]` reverse direction.
    pub reverse_657430: bool,
    /// `[0x628bc0]+4`: nodes per lap.
    pub nodes_per_lap: i32,
    /// `[0x65741c]` player index.
    pub player_65741c: i32,
    /// `[0x606850]` replay camera mode.
    pub camera_606850: i32,
    /// `[0x606a8c]` leave the race loop.
    pub leave_606a8c: bool,
}

/// Lap fraction terms of a node (`0x481b70`): (start, end, length, first node).
pub trait LapTerms {
    fn terms(&self, node: i32) -> (f32, f32, i32, i32);
}

impl LapTerms for super::original_ai::track::TrackNodes {
    fn terms(&self, node: i32) -> (f32, f32, i32, i32) {
        self.progress_terms(node)
    }
}

/// What a progress tick asked of the rest of the game.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TickEvents {
    /// `Car_ResetToTrack(car, 3, 0)` `0x40f540`: behind the start of a point-to-point race.
    pub reset: bool,
    /// `0x4277d0`: the lap speech of the player (it stays silent on the finishing lap).
    pub lap_speech: bool,
    /// The car finished on this tick.
    pub finished: bool,
}

/// The race of all cars: cars in `0x5e8e2c` order (humans first), the order table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RaceTable {
    pub globals: RaceGlobals,
    pub cars: Vec<RaceCar>,
    pub order: [OrderEntry; 8],
}

/// Lap fraction of `node` (`0x416290`, also inline in `0x416310`/`0x4169b0`), extended.
fn fraction_ext(g: &RaceGlobals, terms: &impl LapTerms, node: i32) -> f64 {
    let (start, end, len, first) = terms.terms(node);
    let mut f = (end as f64 - start as f64) * ((node - first) as f64 / len as f64) + start as f64;
    if g.reverse_657430 {
        f = 1.0 - f;
        if !g.linear_657494 && f == 1.0 {
            f = 0.0;
        }
    }
    f
}

/// Lap fraction of a node as stored (`0x416290`, f32).
pub fn fraction(g: &RaceGlobals, terms: &impl LapTerms, node: i32) -> f32 {
    fraction_ext(g, terms, node) as f32
}

/// Fractional part as `0x417440` takes it: `x − ftol(x)` above 0, `x + 1` until ≥ 0 below.
fn frac_part(x: f32) -> f64 {
    let mut v = x as f64;
    if v > 0.0 {
        v - v.trunc()
    } else {
        while v < 0.0 {
            v += 1.0;
        }
        v
    }
}

/// `Car_ProgressLapCount` `0x417440` (race type 0): counts the laps from the progress.
fn lap_count(car: &mut RaceCar, g: &RaceGlobals, ev: &mut TickEvents) {
    let p = car.progress_af8;
    if p == car.progress_af4 {
        return;
    }
    if car.flags_52c & 1 != 0 {
        if g.linear_657494 {
            if p > car.finish_b04 {
                car.laps_530 += 1;
            } else if p < car.behind_b08 {
                ev.reset = true;
            }
        } else {
            let half = 0.5f64;
            let tenth = f32::from_bits(0x3dcc_cccd) as f64;
            if frac_part(p) > half && frac_part(car.progress_af4) < tenth {
                car.back_534 += 1;
            } else if frac_part(p) < tenth && frac_part(car.progress_af4) > half {
                if car.back_534 > 0 {
                    car.back_534 -= 1;
                } else {
                    car.laps_530 += 1;
                }
            }
        }
    }
    car.progress_af4 = p;
}

impl RaceTable {
    pub fn new(globals: RaceGlobals, cars: Vec<RaceCar>) -> Self {
        Self {
            globals,
            cars,
            order: [OrderEntry::default(); 8],
        }
    }

    /// The even-step part of `Sched_Step` 0x414ff0: `0x416140`, then `0x4169b0` for every
    /// car in queue order with its locator node and forward speed `+0xd60`.
    pub fn even_step(
        &mut self,
        step: i32,
        cars: &[(i32, f32)],
        terms: &impl LapTerms,
    ) -> Vec<TickEvents> {
        self.globals.step_606ac4 = step;
        self.clear_order();
        let n = cars.len().min(self.cars.len());
        (0..n)
            .map(|i| {
                self.cars[i].node_08 = cars[i].0;
                self.cars[i].speed_d60 = cars[i].1;
                self.progress_tick(i, terms)
            })
            .collect()
    }

    /// The odd-step part of `Sched_Step` 0x414ff0 after the queue: `0x417010`, `0x415fc0`.
    /// True on the step the race ends.
    pub fn odd_step(&mut self, step: i32, speech_idle: bool) -> bool {
        self.globals.step_606ac4 = step;
        let ended = self.end_check(speech_idle);
        self.places();
        ended
    }

    /// `Race_OrderClear` `0x416140`.
    pub fn clear_order(&mut self) {
        self.order = [OrderEntry::default(); 8];
    }

    /// `Car_TrackProgressTick` `0x4169b0` for car `i` (a racer); call on even steps after
    /// [`Self::clear_order`], every car in queue order.
    pub fn progress_tick(&mut self, i: usize, terms: &impl LapTerms) -> TickEvents {
        let car = &mut self.cars[i];
        car.distance_d00 = (car.speed_d60.abs() as f64 * f32::from_bits(0x39a3_d70a) as f64
            + car.distance_d00 as f64) as f32;
        self.race_progress(i, terms)
    }

    /// `Race_ProgressTick` `0x416310`, single-player branches.
    fn race_progress(&mut self, i: usize, terms: &impl LapTerms) -> TickEvents {
        let mut ev = TickEvents::default();
        let g = self.globals;
        let step = g.step_606ac4;
        let n = self.cars.len();
        let car = &mut self.cars[i];
        let lap = car.laps_b20;
        if lap < 8 && car.speed_d60 > car.top_ad4[lap as usize] {
            car.top_ad4[lap as usize] = car.speed_d60;
        }
        if car.state_b50 == RACING {
            let node = car.node_08;
            if car.node_b18 != node {
                car.nodes_b10 = (car.laps_b20 - car.back_534) * g.nodes_per_lap + node;
                car.node_b18 = node;
                car.step_b14 = step;
                let set = |car: &mut RaceCar| {
                    let f = fraction_ext(&g, terms, node);
                    car.progress_af8 = (car.laps_530 as f64 + f - car.back_534 as f64) as f32;
                };
                set(car);
                lap_count(car, &g, &mut ev);
                set(car);
            }
            if car.laps_b20 != car.laps_530 {
                let lap = car.laps_b20 as usize;
                // `+0xb28[lap]`: index 8 would land on `+0xb48` (8 laps at most).
                if lap < 8 {
                    car.lap_b28[lap] = step - car.time_b24;
                }
                car.time_b24 = step;
                car.laps_b20 = car.laps_530;
                if g.laps_6573f0 <= car.laps_b20 && car.state_b50 == RACING {
                    car.state_b50 = FINISHED;
                    car.step_b14 = step;
                    car.nodes_b10 = (car.laps_b20 + 1) * g.nodes_per_lap;
                    let f = fraction_ext(&g, terms, node);
                    car.progress_af8 = if g.linear_657494 {
                        f as f32
                    } else {
                        (car.laps_b20 as f64 + f) as f32
                    };
                    car.time_b24 -= START_STEPS;
                    ev.finished = true;
                    if car.index_520 == g.player_65741c {
                        self.globals.camera_606850 = CAMERA_TRACK;
                    }
                }
                let car = &self.cars[i];
                if car.human() && car.index_520 == g.player_65741c {
                    ev.lap_speech = true;
                }
            }
        }
        self.insert_order(i, n);
        ev
    }

    /// Order table insertion, the tail of `0x416310`.
    fn insert_order(&mut self, i: usize, n: usize) {
        let car = self.cars[i];
        let n = n.min(8);
        let Some(k) = (0..n).find(|&k| {
            let e = &self.order[k];
            car.progress_af8 > e.progress
                || (car.progress_af8 == e.progress && car.step_b14 < e.step)
        }) else {
            return;
        };
        for j in (k + 1..n).rev() {
            self.order[j] = self.order[j - 1];
        }
        self.order[k] = OrderEntry {
            id: car.index_520,
            progress: car.progress_af8,
            step: car.step_b14,
            human: car.flags_52c & 4,
        };
    }

    /// Rank (1-based) of car `id` in the order table, 0 if absent.
    fn rank(&self, id: i32) -> i32 {
        let n = self.cars.len().min(8);
        self.order[..n]
            .iter()
            .position(|e| e.id == id)
            .map_or(0, |k| k as i32 + 1)
    }

    /// `Race_Places` `0x415fc0` (odd steps): `+0xb48` from the order table.
    pub fn places(&mut self) {
        for k in 0..self.cars.len() {
            self.cars[k].place_b48 = self.rank(self.cars[k].index_520);
        }
    }

    /// `Race_EndCheck` `0x417010` (odd steps): once every human finished more than 5 s
    /// ago (and the speech queue is idle, else 10 s) the race loop ends and
    /// `0x416ac0(2)` computes the results. Returns true on that step.
    pub fn end_check(&mut self, speech_idle: bool) -> bool {
        if self.globals.leave_606a8c {
            return false;
        }
        let step = self.globals.step_606ac4;
        let humans: Vec<&RaceCar> = self.cars.iter().filter(|c| c.human()).collect();
        let done = humans
            .iter()
            .filter(|c| {
                c.state_b50 > 1
                    && c.step_b14 + 0x140 < step
                    && (speech_idle || c.step_b14 + 0x280 < step)
            })
            .count();
        if done == humans.len() {
            self.globals.leave_606a8c = true;
            self.final_results(2);
            return true;
        }
        false
    }

    /// `Race_FinalResults` `0x416ac0`: `mode` 2 at the race end, 1 when the player leaves.
    /// Humans first, then the other cars: the cars still racing get an estimated time.
    pub fn final_results(&mut self, mode: i32) {
        let g = self.globals;
        let laps = g.laps_6573f0;
        let human0_b00 = self
            .cars
            .iter()
            .find(|c| c.human())
            .map_or(0.0, |c| c.start_b00);
        for k in 0..self.cars.len() {
            let id = self.cars[k].index_520;
            let rank = self.rank(id);
            let car = &mut self.cars[k];
            if car.human() {
                if car.state_b50 == ABANDONED {
                    car.time_b24 = estimate_linear(car, car.start_b00, laps);
                    car.state_b50 = ABANDONED;
                }
                if mode == 1 {
                    car.state_b50 = ABANDONED;
                    car.laps_cdc = car.lap_b28;
                }
                car.place_cd4 = rank;
                car.time_cd8 = car.time_b24;
                car.result_cd0 = car.state_b50;
                car.best_cfc = car.lap_b28[0];
                copy_laps(car, laps);
            } else {
                if car.state_b50 == RACING {
                    let t = if !g.linear_657494 {
                        round_even(
                            (car.step_b14 - START_STEPS) as f64
                                / (car.progress_af8 as f64 / laps as f64),
                        )
                    } else {
                        estimate_linear(car, human0_b00, laps)
                    };
                    car.time_b24 = t;
                    car.state_b50 = FINISHED;
                    if car.lap_b28[0] == 0 {
                        car.lap_b28[0] = t;
                    }
                }
                car.place_cd4 = rank;
                car.time_cd8 = car.time_b24;
                car.result_cd0 = car.state_b50;
                car.best_cfc = 99999;
                for l in 0..laps.clamp(0, 8) as usize {
                    if car.lap_b28[l] < car.best_cfc && car.lap_b28[l] > 0 {
                        car.best_cfc = car.lap_b28[l];
                    }
                }
                copy_laps(car, laps);
            }
        }
    }
}

/// `+0xcdc[lap] = +0xb28[lap]` and the best lap from the non-zero ones.
fn copy_laps(car: &mut RaceCar, laps: i32) {
    for l in 0..laps.clamp(0, 8) as usize {
        car.laps_cdc[l] = car.lap_b28[l];
        let t = car.laps_cdc[l];
        if t < car.best_cfc && t != 0 {
            car.best_cfc = t;
        }
    }
}

/// Estimated race time of a point-to-point car (`0x416ac0`): the time so far scaled by
/// the part of the course driven, from `b00` (a human's start fraction) to `+0xb04`.
fn estimate_linear(car: &RaceCar, b00: f32, laps: i32) -> i32 {
    let mut done = car.progress_af8 as f64 - b00 as f64;
    if done <= 0.001f32 as f64 {
        done = 0.001f32 as f64;
    }
    let part = done / (car.finish_b04 as f64 - b00 as f64);
    let mut t = (car.step_b14 - START_STEPS) as f64;
    if part < 0.005f32 as f64 {
        t = part * 250000.0;
    }
    round_even(laps as f64 * t / part)
}

/// `fistp` with the default rounding (to nearest, ties to even).
fn round_even(x: f64) -> i32 {
    x.round_ties_even() as i32
}

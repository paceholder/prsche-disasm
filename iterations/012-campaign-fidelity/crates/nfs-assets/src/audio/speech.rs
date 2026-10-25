//! Original NFS5 race speech (announcer): phrase choice, countdown, lap and result calls and
//! the request queue, without an audio backend.
//!
//! Research notes: `docs/re/audio.md` "race speech" in the research workspace. Phrase `i` is
//! the `i`-th name of `oneshots.txt` (`nfs_formats::parse_oneshots`). `rng` stands for the
//! original `0x538040()`.
use std::collections::VecDeque;

/// `g_CountdownSteps` 0x5cc7c4: scheduler steps (64 Hz) of "3", "2", "1", "Go".
pub const COUNTDOWN_STEPS: [i32; 4] = [0x100, 0x140, 0x180, 0x1c0];
/// `Sched_Step` 0x414ff0: the start is over after step 0x1bf; 0x1c0 releases the car.
pub const START_STEP: i32 = 0x1c0;
/// Scheduler rate (`docs/re/vehicle.md`).
pub const SCHED_HZ: f32 = 64.0;
/// Countdown length in seconds: `START_STEP / SCHED_HZ` = 7.
pub const START_SECS: f32 = START_STEP as f32 / SCHED_HZ;
/// Requests waiting at most (`Speech_Enqueue` 0x4b05a0 refuses above 32 + the one being added).
pub const QUEUE_MAX: usize = 33;
/// `Race_GapSeconds` 0x435460: gaps are valid above this player speed, m/s.
pub const GAP_MIN_SPEED: f32 = 13.39;

type Rng<'a> = &'a mut dyn FnMut() -> u32;

fn r1(rng: Rng) -> u16 {
    (rng() & 1) as u16
}

/// `Say_Countdown` 0x4a6240.
pub fn say_countdown(k: u16) -> u16 {
    k
}
/// `Say_RacePlacing` 0x4a6430, `p` = place − 1.
pub fn say_race_placing(p: u16, rng: Rng) -> u16 {
    match p {
        0 => 7 + r1(rng),
        7 => 15 + r1(rng),
        _ => 8 + p,
    }
}
/// `Say_LostRace` 0x4a62a0.
pub fn say_lost_race(rng: Rng) -> u16 {
    17 + r1(rng)
}
/// `Say_PositionRel` 0x4a61a0, `lead` = seconds ahead of the nearest car.
pub fn say_position_rel(lead: i32) -> u16 {
    38 + match lead {
        31.. => 0,
        13..=30 => 1,
        7..=12 => 2,
        6 => 3,
        5 => 4,
        4 => 5,
        3 => 6,
        2 => 7,
        1 => 8,
        -4..=0 => 9,
        _ => 10,
    }
}
/// `Say_BestLap` 0x4a6580.
pub fn say_best_lap(rng: Rng) -> u16 {
    49 + (rng() & 3) as u16
}
/// `Say_LapPosition` 0x4a6390, `p` = place − 1.
pub fn say_lap_position(p: u16, rng: Rng) -> u16 {
    let i = 53 + p + 9 * r1(rng);
    if p == 7 {
        i + r1(rng)
    } else {
        i
    }
}
/// `Say_LapTimeBehind` 0x4a63e0: `None` for `s <= 0`.
pub fn say_lap_time_behind(s: i32, rng: Rng) -> Option<u16> {
    (s > 0).then(|| 71 + s.min(12) as u16 + 13 * r1(rng))
}
/// `Say_KnockoutQualify` 0x4a6320.
pub fn say_knockout_qualify() -> u16 {
    97
}
/// `Say_KnockoutWon` 0x4a62e0.
pub fn say_knockout_won(rng: Rng) -> u16 {
    98 + r1(rng)
}
/// `Say_KnockoutEliminated` 0x4a6350.
pub fn say_knockout_eliminated(rng: Rng) -> u16 {
    100 + (rng() % 7) as u16
}
/// `Say_FinalLap` 0x4a65c0.
pub fn say_final_lap(rng: Rng) -> u16 {
    107 + (rng() % 6) as u16
}
/// `Say_Lap` 0x4a6640, `k` = laps done − 1.
pub fn say_lap(k: u16, rng: Rng) -> u16 {
    132 + k + 8 * r1(rng)
}

/// `Audio_SpeechVolume` 0x4a8300 / `Audio_CountdownVolume` 0x4a8320 (`0x5d14b4`, `0x5d14c0`
/// are both 127): `speech × 127 × 0x55 >> 14`, 0..127.
pub fn speech_volume(speech_level: i32) -> i32 {
    (speech_level.clamp(0, 127) * 127 * 0x55) >> 14
}

/// Countdown part of `Sched_Step` 0x414ff0 (`g_CountdownNext` 0x656858).
#[derive(Debug, Clone, Default)]
pub struct Countdown {
    next: usize,
}

impl Countdown {
    /// Phrases due at scheduler step `step`. `quiet` = no countdown (replay `0x657424 >= 2`,
    /// race types 6, 7, 8). Once per step; a late call catches up phrase by phrase.
    pub fn update(&mut self, step: i32, quiet: bool) -> Option<u16> {
        if quiet || step > START_STEP {
            return None;
        }
        let k = self.next;
        (k < 4 && step >= COUNTDOWN_STEPS[k]).then(|| {
            self.next += 1;
            say_countdown(k as u16)
        })
    }
}

/// Inputs of `Race_OnLapSpeech` 0x4277d0 after a completed lap.
#[derive(Debug, Clone, Copy)]
pub struct LapEvent {
    /// Laps completed, including this one (`0x60ca90`).
    pub laps_done: u32,
    /// `0x6573f0`.
    pub total_laps: u32,
    /// `0x5e8ae8`.
    pub cars: u32,
    /// Player place, 1-based (`car+0xb48`).
    pub place: u32,
    /// This lap is faster than all earlier ones (from the second lap on).
    pub new_best: bool,
    /// `Race_GapSeconds` 0x435460: seconds the nearest car ahead leads (positive = someone
    /// is ahead), `None` when not valid (player at or below 13.39 m/s, or not computed).
    pub gap: Option<i32>,
    /// Race type `0x6573e8` (0 = plain race, 1 = knockout).
    pub race_type: u32,
    /// Replay (`0x657424` 2 or 3) or `0x657494 != 0`.
    pub quiet: bool,
}

/// `Race_OnLapSpeech` 0x4277d0: phrases in request order.
pub fn on_lap_completed(e: &LapEvent, rng: Rng) -> Vec<u16> {
    let mut out = Vec::new();
    let finished = e.laps_done == e.total_laps;
    let knocked_out = e.race_type == 1 && (e.cars as i64 - e.laps_done as i64) < e.place as i64;
    if e.quiet || knocked_out || finished || e.laps_done == 0 {
        return out;
    }
    out.push(say_lap((e.laps_done - 1) as u16, rng));
    if e.cars > 1 {
        if e.cars == 2 {
            if let Some(gap) = e.gap {
                out.push(say_position_rel(-gap));
            }
        } else {
            if e.place >= 1 && e.place <= e.cars {
                out.push(say_lap_position((e.place - 1) as u16, rng));
            }
            if let Some(gap) = e.gap {
                out.extend(say_lap_time_behind(gap, rng));
            }
        }
    }
    if e.new_best {
        out.push(say_best_lap(rng));
    }
    if e.laps_done + 1 == e.total_laps {
        out.push(say_final_lap(rng));
    }
    out
}

/// Speech part of `RaceHud_PositionTick` 0x41d050 on the first tick after the finish.
pub fn on_race_finished(place: u32, cars: u32, race_type: u32, rng: Rng) -> Vec<u16> {
    let place = place.max(1);
    match race_type {
        1 if place == 1 => vec![say_knockout_won(rng)],
        1 => vec![say_knockout_eliminated(rng)],
        2 if place == 1 && cars == 2 => vec![say_knockout_won(rng)],
        2 if place >= cars => vec![say_knockout_eliminated(rng)],
        2 => vec![say_knockout_qualify()],
        _ => {
            let mut v = vec![say_race_placing((place - 1).min(9) as u16, rng)];
            if place == cars && cars > 1 {
                v.push(say_lost_race(rng));
            }
            v
        }
    }
}

/// Result music of 0x41d050: `win*` for 1st place, `lose*` otherwise, only with more than
/// one car (`Music_Play(pattern, 1)`).
pub fn result_music(place: u32, cars: u32) -> Option<&'static str> {
    (cars > 1).then_some(if place == 1 { "win*" } else { "lose*" })
}

/// Request ring of `Speech_Enqueue` 0x4b05a0 and `Speech_Update` 0x4b0a20: phrases play in
/// request order, one at a time.
#[derive(Debug, Clone, Default)]
pub struct SpeechQueue {
    waiting: VecDeque<u16>,
    playing: Option<u16>,
}

impl SpeechQueue {
    /// Adds a request; `false` when the queue is full or the phrase is missing (size 0).
    pub fn push(&mut self, phrase: u16, available: bool) -> bool {
        if !available || self.waiting.len() >= QUEUE_MAX {
            return false;
        }
        self.waiting.push_back(phrase);
        true
    }

    /// Per frame. `remaining_cs` = time left of the playing phrase in 1/100 s (`None` when
    /// it has ended). A phrase counts as finished below 5/100 s (0x4b0a20). Returns the
    /// phrase to start now.
    pub fn update(&mut self, remaining_cs: Option<i32>) -> Option<u16> {
        if self.playing.is_some() && remaining_cs.is_some_and(|r| r >= 5) {
            return None;
        }
        self.playing = self.waiting.pop_front();
        self.playing
    }

    pub fn clear(&mut self) {
        self.waiting.clear();
        self.playing = None;
    }

    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant(v: u32) -> impl FnMut() -> u32 {
        move || v
    }

    #[test]
    fn countdown_steps() {
        let mut c = Countdown::default();
        let mut fired = Vec::new();
        for step in 0..=START_STEP + 10 {
            if let Some(p) = c.update(step, false) {
                fired.push((step, p));
            }
        }
        assert_eq!(fired, [(0x100, 0), (0x140, 1), (0x180, 2), (0x1c0, 3)]);
        assert_eq!(START_SECS, 7.0);
        assert_eq!(Countdown::default().update(0x150, true), None);
    }

    #[test]
    fn index_rules() {
        let (mut a, mut b) = (constant(0), constant(1));
        assert_eq!(
            (say_race_placing(0, &mut a), say_race_placing(0, &mut b)),
            (7, 8)
        );
        assert_eq!(say_race_placing(2, &mut a), 10); // you placed 3rd
        assert_eq!(
            (say_race_placing(7, &mut a), say_race_placing(7, &mut b)),
            (15, 16)
        );
        assert_eq!(
            (say_lap_position(2, &mut a), say_lap_position(2, &mut b)),
            (55, 64)
        );
        assert_eq!(say_lap_position(7, &mut b), 70); // you're in last
        assert_eq!((say_lap(1, &mut a), say_lap(1, &mut b)), (133, 141));
        assert_eq!(say_lap_time_behind(1, &mut a), Some(72));
        assert_eq!(say_lap_time_behind(40, &mut b), Some(96));
        assert_eq!(say_lap_time_behind(0, &mut a), None);
        assert_eq!(
            [31, 13, 7, 6, 1, 0, -4, -5].map(say_position_rel),
            [38, 39, 40, 41, 46, 47, 47, 48]
        );
        assert_eq!(say_best_lap(&mut constant(7)), 52);
        assert_eq!(say_final_lap(&mut constant(13)), 108);
        assert_eq!(say_knockout_eliminated(&mut constant(13)), 106);
        assert_eq!(speech_volume(127), 83);
    }

    #[test]
    fn lap_calls_in_original_order() {
        // Lap 2 of 3 done in 3rd place of 4 with a new best lap.
        let e = LapEvent {
            laps_done: 2,
            total_laps: 3,
            cars: 4,
            place: 3,
            new_best: true,
            gap: None,
            race_type: 0,
            quiet: false,
        };
        assert_eq!(on_lap_completed(&e, &mut constant(0)), [133, 55, 49, 107]);
        // The finishing lap is silent; the result call takes over.
        let last = LapEvent { laps_done: 3, ..e };
        assert!(on_lap_completed(&last, &mut constant(0)).is_empty());
        let solo = LapEvent {
            laps_done: 1,
            cars: 1,
            new_best: false,
            ..e
        };
        assert_eq!(on_lap_completed(&solo, &mut constant(0)), [132]);
        let two = LapEvent {
            cars: 2,
            gap: Some(-3),
            new_best: false,
            ..e
        };
        assert_eq!(on_lap_completed(&two, &mut constant(0)), [133, 44, 107]);
    }

    #[test]
    fn finish_calls() {
        assert_eq!(on_race_finished(1, 4, 0, &mut constant(1)), [8]);
        assert_eq!(on_race_finished(4, 4, 0, &mut constant(0)), [10 + 1, 17]);
        assert_eq!(on_race_finished(1, 1, 0, &mut constant(0)), [7]);
        assert_eq!(result_music(1, 4), Some("win*"));
        assert_eq!(result_music(2, 4), Some("lose*"));
        assert_eq!(result_music(1, 1), None);
    }

    #[test]
    fn queue_plays_back_to_back() {
        let mut q = SpeechQueue::default();
        for p in [133, 55, 49, 107] {
            assert!(q.push(p, true));
        }
        assert!(!q.push(1, false));
        assert_eq!(q.update(None), Some(133));
        assert_eq!(q.update(Some(40)), None);
        assert_eq!(q.update(Some(4)), Some(55));
        assert_eq!(q.update(None), Some(49));
        assert_eq!(q.update(None), Some(107));
        assert_eq!(q.update(None), None);
        for _ in 0..40 {
            q.push(0, true);
        }
        assert_eq!(q.waiting(), QUEUE_MAX);
    }
}

//! Keyboard steering, raw key state to front wheel angle, against the
//! original chain run in Unicorn (scripts/research/replay-vehicle-keyboard.py):
//! the 128 Hz timer callback `0x4a45d0` polls `Input_PollControllers`
//! `0x412e70` (Key getter `0x411c20` on the scancode table `0x5de028`) every
//! 4th tick and queues the word (`0x413800` -> `0x415e80`); each even 64 Hz
//! step pops its slot (`0x415ef0`), `Car_ReadInputs` `0x4991e0` gets it
//! through the player callback `0x414810` -> `0x4143f0` -> `0x413870` into
//! `g_InputWord` `0x5e9664`, then the V5 step (`Car_ApplyControls` steering
//! ramp, `Car_PlanarDynamics` wheel steer `w+0xb4`, smoothed `+0xac8`).
//!
//! The port gets only the key state of each polling tick: `input_word`
//! packs it, the word of poll n drives 64 Hz step 2n (slot 0, before the
//! first poll, is 0, as `0x415e00` leaves it), and `step_64hz` runs the car.
//! Tolerance: none (bit for bit), as V4/V5.

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
#[allow(unused_imports)]
use local_fixtures::Fixture;

#[path = "support/vehicle_step.rs"]
mod fixture;

use fixture::*;
use nfs_assets::physics::original_vehicle::input_word;
use nfs_assets::physics::original_vehicle::step::{step_64hz, GroundContact, Inputs};

static KB_RUNS: Fixture = Fixture::new("vehicle-keyboard-runs.tsv");
static KB_TICKS: Fixture = Fixture::new("vehicle-keyboard-ticks.tsv");
static KB_DYN: Fixture = Fixture::new("vehicle-keyboard-dyn.tsv");
static KB_BODY: Fixture = Fixture::new("vehicle-keyboard-body.tsv");

/// The engine's word for one key state (what `run_original_vehicle` builds
/// from the held keys: steer = right - left, gas, brake, handbrake).
///
/// A held Shift makes the original Key getter `0x411c20` report every
/// binding without the Shift flag as released (run `shift_right_20`); the
/// engine does not model modifiers (approximation A200), so the test
/// releases the keys itself while Shift is down.
fn word_for_keys(r: &Row) -> u32 {
    let shift = r.i("shift") != 0;
    let k = |name: &str| if shift { 0.0 } else { r.i(name) as f32 };
    input_word(
        k("gas"),
        k("brake"),
        k("right") - k("left"),
        !shift && r.i("handbrake") != 0,
    )
}

#[test]
fn keyboard_chain_matches_the_original_bit_for_bit() {
    skip_without_local_fixtures!(CARS, KB_RUNS, KB_TICKS, KB_DYN, KB_BODY);
    let specs = cars();
    let ticks = rows(&KB_TICKS);
    let dyn_rows = rows(&KB_DYN);
    let body = rows(&KB_BODY);
    let (mut polls, mut steps32) = (0, 0);
    let mut max_d94_seen = 0;
    for run in rows(&KB_RUNS) {
        let name = run.s("run");
        // Queue: slot 0 = 0, slot n = poll n (the key state of that tick).
        let mut queue = vec![0u32];
        for t in ticks.iter().filter(|t| t.s("run") == name) {
            if t.i("polled") != 0 {
                let w = word_for_keys(t);
                assert_eq!(
                    w as i64,
                    t.i("poll_word"),
                    "{name} tick {}: word {w:#x}, original {:#x}",
                    t.s("tick"),
                    t.i("poll_word")
                );
                queue.push(w);
                polls += 1;
            }
        }
        let d: Vec<&Row> = dyn_rows.iter().filter(|r| r.s("run") == name).collect();
        let b: Vec<&Row> = body.iter().filter(|r| r.s("run") == name).collect();
        let (mut car, mut lcg) = initial(&specs[run.s("car")], &run);
        car.flags_52c = run.i("flags_52c") as u8;
        let steps = run.i("steps") as usize;
        assert_eq!(b.len(), steps, "{name}");
        let mut k32 = 0;
        for n in 0..steps {
            let even = n % 2 == 0;
            let word = if even { queue[n / 2] } else { 0 };
            step_64hz(
                &mut car,
                &Inputs {
                    input_word: word,
                    ground: GroundContact::FLAT,
                },
                &mut lcg,
            );
            if even {
                let r = d[k32];
                assert_eq!(r.i("step") as usize, n, "{name}");
                assert_eq!(r.i("popped"), word as i64, "{name} step {n}: popped word");
                assert_eq!(
                    r.i("input_word"),
                    word as i64,
                    "{name} step {n}: g_InputWord"
                );
                for (k, v) in [
                    ("gas_d7c", car.gas_d7c as i64),
                    ("brake_d7d", car.brake_d7d as i64),
                    ("steer_in_d90", car.steer_in_d90 as i64),
                ] {
                    assert_eq!(r.i(k), v, "{name} step {n}: {k}");
                }
                for (k, v) in dyn_values(&car) {
                    assert!(
                        same(&v, r, &k),
                        "{name} step {n}: {k} = {v:?}, original {}",
                        r.s(&k)
                    );
                }
                max_d94_seen = max_d94_seen.max(car.steer_d94.abs());
                k32 += 1;
                steps32 += 1;
            }
            for (k, v) in body_values(&car) {
                assert_eq!(
                    b[n].f(&k).to_bits(),
                    v.to_bits(),
                    "{name} step {n}: {k} = {v:?}, original {}",
                    b[n].s(&k)
                );
            }
        }
        assert_eq!(k32, d.len(), "{name}");
    }
    assert_eq!((polls, steps32), (679, 688));
    assert_eq!(max_d94_seen, 126);
}

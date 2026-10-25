//! Browser-independent core of the original tyre squeal and surface rumble: decodes the patches
//! of `SND_coll.bnk` that `TyreSounds` plays and steps it at the 32 Hz effects tick. The wasm
//! export (`browser::TyreAudio`) is a thin wrapper over [`TyreAudioCore`]
//! (`docs/re/audio.md`, "Tyre and surface sounds").
use std::collections::BTreeMap;

use nfs_assets::audio::{
    control_cents, tyre_slots, EngineView, TyreCommand, TyreInputs, TyreSounds, TYRE_TICK_HZ,
};
use nfs_formats::parse_bnk;

use crate::engine_audio::{decode_slot, SlotPcm, COMMAND_STRIDE};

pub use crate::engine_audio::{CMD_SET_PITCH, CMD_SET_VOLUME, CMD_START, CMD_STOP};

/// Floats per car in [`TyreAudioCore::update`] (see [`inputs_from_slice`]).
pub const INPUT_LEN: usize = 16;
/// Upper bound of ticks per `update` call.
pub const MAX_TICKS_PER_CALL: u32 = 4;

/// `TyreInputs` from the flat layout `get_tyre_sound_inputs` returns: skids front 0, 1, rear 0,
/// 1; `car+0x78`; height `+0x424`; ground normal `+0x3a0`; surface `+0x464`; `+0x540`; tyre
/// type; `+0x52c` bit 4; `d60`; rpm; redline; wheels off > 5; `res+0xc == 1`.
pub fn inputs_from_slice(v: &[f32]) -> TyreInputs {
    let g = |i: usize| v.get(i).copied().unwrap_or(0.0);
    TyreInputs {
        skid_front: [g(0), g(1)],
        skid_rear: [g(2), g(3)],
        field_78: g(4),
        height_424: g(5),
        ground_up_3a0: if v.len() > 6 { g(6) } else { 1.0 },
        surface_464: g(7) as i32,
        surface_override_540: g(8) != 0.0,
        tyre_type_1c0: g(9) as i32,
        flags_52c_bit4: g(10) != 0.0,
        vel_fwd_d60: g(11),
        rpm_db4: g(12),
        redline_94: if v.len() > 13 { g(13) } else { 7000.0 },
        wheels_off_gt5: g(14) != 0.0,
        res_c_is_1: g(15) != 0.0,
    }
}

pub struct TyreAudioCore {
    sounds: TyreSounds,
    slots: Vec<SlotPcm>,
    accumulator: f32,
    ticks: u64,
    starts: u64,
}

fn find_by_basename<'a>(files: &'a BTreeMap<String, Vec<u8>>, base: &str) -> Option<&'a [u8]> {
    files
        .iter()
        .find(|(name, _)| {
            name.replace('\\', "/")
                .rsplit('/')
                .next()
                .is_some_and(|b| b.eq_ignore_ascii_case(base))
        })
        .map(|(_, d)| d.as_slice())
}

impl TyreAudioCore {
    /// `files` must contain `SND_coll.bnk` (any directory prefix).
    pub fn load(files: &BTreeMap<String, Vec<u8>>) -> Result<Self, String> {
        let bank = find_by_basename(files, "SND_coll.bnk").ok_or("нет файла SND_coll.bnk")?;
        let parsed = parse_bnk(bank).map_err(|e| format!("SND_coll.bnk: {e}"))?;
        let mut slots = Vec::new();
        for id in tyre_slots() {
            if let Some(slot) = decode_slot(bank, &parsed, id)? {
                slots.push(slot);
            }
        }
        Ok(Self {
            sounds: TyreSounds::new(),
            slots,
            accumulator: 0.0,
            ticks: 0,
            starts: 0,
        })
    }

    pub fn slots(&self) -> &[SlotPcm] {
        &self.slots
    }

    fn bend(&self, slot: u8) -> i32 {
        self.slots
            .iter()
            .find(|s| s.id == slot)
            .map_or(0, |s| s.bend_range)
    }

    /// 4.12 pitch of a voice: `pitch × 2^(controller cents / 1200)`.
    fn pitch(&self, slot: u8, control: u8, pitch: u16) -> i32 {
        let cents = control_cents(control, self.bend(slot));
        (pitch as f64 * 2f64.powf(cents as f64 / 1200.0)).round() as i32
    }

    /// Advances by `dt` seconds in 1/32 s ticks with the car state of the frame and returns
    /// commands, `[kind, voice, slot, volume, pitch, fade_ticks, looped]` each. `view`: 0
    /// Chase, 1 Bumper, 2 Free.
    pub fn update(&mut self, dt: f32, inputs: &[f32], view: u8) -> Vec<i32> {
        self.accumulator += dt.clamp(0.0, 1.0);
        let step = 1.0 / TYRE_TICK_HZ;
        let car = inputs_from_slice(inputs);
        let view = match view {
            1 => EngineView::Bumper,
            2 => EngineView::Free,
            _ => EngineView::Chase,
        };
        let mut out = Vec::new();
        let mut n = 0;
        while self.accumulator >= step && n < MAX_TICKS_PER_CALL {
            self.accumulator -= step;
            n += 1;
            self.ticks += 1;
            for c in self.sounds.tick(&car, view) {
                self.flatten(c, &mut out);
            }
        }
        if n == MAX_TICKS_PER_CALL {
            self.accumulator = 0.0;
        }
        out
    }

    fn flatten(&mut self, c: TyreCommand, out: &mut Vec<i32>) {
        match c {
            TyreCommand::Start {
                voice,
                slot,
                volume,
                control,
                pitch,
                ..
            } => {
                self.starts += 1;
                let looped =
                    self.slots
                        .iter()
                        .any(|s| s.id == slot && s.loop_range.is_some()) as i32;
                out.extend([
                    CMD_START,
                    voice as i32,
                    slot as i32,
                    volume as i32,
                    self.pitch(slot, control, pitch),
                    0,
                    looped,
                ]);
            }
            TyreCommand::Update {
                voice,
                slot,
                volume,
                control,
                pitch,
                ..
            } => {
                out.extend([CMD_SET_VOLUME, voice as i32, 0, volume as i32, 0, 0, 0]);
                out.extend([
                    CMD_SET_PITCH,
                    voice as i32,
                    0,
                    0,
                    self.pitch(slot, control, pitch),
                    0,
                    0,
                ]);
            }
            TyreCommand::Stop { voice, fade_ticks } => {
                out.extend([CMD_STOP, voice as i32, 0, 0, 0, fade_ticks as i32, 0]);
            }
        }
    }

    /// Stops every voice (returns the stop commands) and forgets the squeal bits.
    pub fn silence(&mut self) -> Vec<i32> {
        let mut out = Vec::new();
        for c in self.sounds.silence() {
            self.flatten(c, &mut out);
        }
        out
    }

    /// Playing voices as `[channel, slot, volume, control, pitch]` (channel 0 front squeal, 1
    /// rear squeal, 2 rumble), then `[ticks, starts]` as the last two entries.
    pub fn state(&self) -> Vec<i32> {
        let mut v = Vec::new();
        for a in self.sounds.active() {
            v.extend([
                a.channel as i32,
                a.slot as i32,
                a.volume as i32,
                a.control as i32,
                self.pitch(a.slot, a.control, a.pitch),
            ]);
        }
        v.push(self.ticks as i32);
        v.push(self.starts as i32);
        v
    }
}

const _: () = assert!(COMMAND_STRIDE == 7);

#[cfg(test)]
mod tests {
    use super::*;

    fn game_sounds() -> Option<std::path::PathBuf> {
        let mut dir = std::env::current_dir().ok()?;
        loop {
            let g = dir.join("game/app/GameData/Sounds");
            if g.is_dir() {
                return Some(g);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    fn core() -> Option<TyreAudioCore> {
        let dir = game_sounds()?;
        let mut files = BTreeMap::new();
        files.insert(
            "gamedata/sounds/snd_coll.bnk".to_string(),
            std::fs::read(dir.join("SND_coll.bnk")).unwrap(),
        );
        Some(TyreAudioCore::load(&files).unwrap())
    }

    #[test]
    fn missing_bank_is_reported() {
        let err = TyreAudioCore::load(&BTreeMap::new()).err().unwrap();
        assert!(err.contains("SND_coll.bnk"), "{err}");
    }

    #[test]
    fn installed_bank_decodes_the_tyre_slots() {
        let Some(core) = core() else { return };
        let ids: Vec<u8> = core.slots().iter().map(|s| s.id).collect();
        for want in (0x11..=0x1c).chain(0x28..=0x2f) {
            assert!(ids.contains(&want), "slot {want:#x}");
        }
        assert!(!ids.contains(&0x1d), "slot 0x1d is empty in the bank");
        // Squeal slots loop; slot 0x2a has a pitch range.
        for id in 0x28..=0x2f {
            assert!(core
                .slots()
                .iter()
                .find(|s| s.id == id)
                .unwrap()
                .loop_range
                .is_some());
        }
        assert_eq!(
            core.slots()
                .iter()
                .find(|s| s.id == 0x2a)
                .unwrap()
                .bend_range,
            5
        );
    }

    #[test]
    fn slip_starts_the_squeal_voice() {
        let Some(mut core) = core() else { return };
        let mut v = vec![0.0f32; INPUT_LEN];
        v[6] = 1.0; // ground normal
        v[7] = 1.0; // surface
        v[13] = 7000.0;
        v[11] = 30.0; // speed
        let cmds = core.update(0.1, &v, 0);
        assert!(cmds
            .chunks(COMMAND_STRIDE)
            .all(|c| c[0] != CMD_START || c[2] < 0x28));
        v[0] = 0.4;
        v[1] = 0.4;
        let cmds = core.update(0.05, &v, 0);
        let start = cmds
            .chunks(COMMAND_STRIDE)
            .find(|c| c[0] == CMD_START && c[2] == 0x2a)
            .expect("front squeal");
        assert_eq!(start[6], 1, "looped");
        // Controller 10: (10 - 64) * 5 * 100 >> 6 = -422 cents.
        let want = (4096.0 * 2f64.powf(-422.0 / 1200.0)).round() as i32;
        assert_eq!(start[4], want);
        assert!(core.state().len() > 2);
        v[0] = 0.0;
        v[1] = 0.0;
        let cmds = core.update(0.05, &v, 0);
        assert!(cmds
            .chunks(COMMAND_STRIDE)
            .any(|c| c[0] == CMD_STOP && c[5] == 5));
    }
}

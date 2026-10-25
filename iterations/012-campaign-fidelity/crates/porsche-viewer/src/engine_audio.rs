//! Browser-independent core of the original engine sound: loads an engine bank and `.spl`,
//! decodes the slots in use to PCM and steps `EngineMixer` at a fixed rate. The wasm export
//! (`browser::EngineAudio`) is a thin wrapper over [`EngineAudioCore`].
//!
//! One-shot clock (start/stop samples): answered here, not by JS. The core counts mixer
//! steps (1 step = 1/100 s = 1 cs, so `time_remaining_cs` is exact) and remembers when each
//! one-shot `Start` was issued; a one-shot is finished once its decoded length at the sample's
//! own rate (pitch 0x1000) has elapsed in step time, or when it is unknown or was stopped.
//! The step time follows the render loop, not the audio clock, so a stalled tab stalls the
//! one-shots with it (approximation).
use std::collections::{BTreeMap, BTreeSet, HashMap};

use nfs_assets::audio::{
    EngineMixer, EngineSoundInputs, EngineTelemetry, EngineView, MixerGlobals, OneShotClock,
    VoiceCommand, VoiceHost, VoiceId, engine_tables_from_viv,
};
use nfs_formats::{
    decode_patch, find_car, parse_bnk, parse_car_catalogue, parse_spl, parse_viv, viv_file,
};

/// Mixer steps per second (same value as `nfs_assets::audio::UPDATE_HZ`).
pub const STEP_HZ: f32 = 100.0;
/// Upper bound of steps per `update` call, so a long frame doesn't burst hundreds of updates.
pub const MAX_STEPS_PER_CALL: u32 = 10;
/// Values per command in [`EngineAudioCore::update`] output.
pub const COMMAND_STRIDE: usize = 7;
/// Start/stop one-shot slots (`0x4ac290`): 6..=8 start, 9..=11 stop, by view mode.
const ONE_SHOT_SLOTS: std::ops::RangeInclusive<u8> = 6..=11;

/// One decoded bank slot, planar f32 PCM.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotPcm {
    pub id: u8,
    pub sample_rate: u32,
    /// Base detune at the default note: `(60 - root) * 100 + fine` cents.
    pub detune_cents: i32,
    /// Loop points in sample frames.
    pub loop_range: Option<(u32, u32)>,
    pub channels: Vec<Vec<f32>>,
}

/// Command kinds in the flat output of [`EngineAudioCore::update`].
pub const CMD_START: i32 = 0;
pub const CMD_SET_VOLUME: i32 = 1;
pub const CMD_SET_PAN: i32 = 2;
pub const CMD_SET_PITCH: i32 = 3;
pub const CMD_STOP: i32 = 4;

/// Flattens a command to `[kind, voice, slot, volume, pitch, pan, looped]`; unused fields are 0.
/// Pans are signed (`pan as i16`).
pub fn flatten_command(c: &VoiceCommand) -> [i32; COMMAND_STRIDE] {
    match *c {
        VoiceCommand::Start {
            voice,
            slot,
            volume,
            pitch,
            pan,
            looped,
        } => [
            CMD_START,
            voice.0 as i32,
            slot as i32,
            volume as i32,
            pitch as i32,
            pan as i32,
            looped as i32,
        ],
        VoiceCommand::SetVolume { voice, volume } => {
            [CMD_SET_VOLUME, voice.0 as i32, 0, volume as i32, 0, 0, 0]
        }
        VoiceCommand::SetPan { voice, pan } => {
            [CMD_SET_PAN, voice.0 as i32, 0, 0, 0, pan as i16 as i32, 0]
        }
        VoiceCommand::SetPitch { voice, pitch } => {
            [CMD_SET_PITCH, voice.0 as i32, 0, 0, pitch as i32, 0, 0]
        }
        VoiceCommand::Stop { voice } => [CMD_STOP, voice.0 as i32, 0, 0, 0, 0, 0],
    }
}

/// Step-time clock for the start/stop samples.
#[derive(Default)]
struct StepClock {
    now_cs: i64,
    /// One-shot length in cs by slot.
    length_cs: HashMap<u8, i64>,
    /// Active one-shots: end time in cs.
    ends: HashMap<VoiceId, i64>,
}

impl OneShotClock for StepClock {
    fn is_finished(&mut self, voice: VoiceId) -> bool {
        self.ends.get(&voice).is_none_or(|&end| self.now_cs >= end)
    }
    fn time_remaining_cs(&mut self, voice: VoiceId) -> i32 {
        self.ends
            .get(&voice)
            .map_or(0, |&end| (end - self.now_cs).max(0) as i32)
    }
}

struct Host<'a> {
    clock: &'a mut StepClock,
    out: &'a mut Vec<VoiceCommand>,
}

impl OneShotClock for Host<'_> {
    fn is_finished(&mut self, voice: VoiceId) -> bool {
        self.clock.is_finished(voice)
    }
    fn time_remaining_cs(&mut self, voice: VoiceId) -> i32 {
        self.clock.time_remaining_cs(voice)
    }
}

impl VoiceHost for Host<'_> {
    fn command(&mut self, command: VoiceCommand) {
        match command {
            VoiceCommand::Start {
                voice,
                slot,
                looped: false,
                ..
            } => {
                let len = self.clock.length_cs.get(&slot).copied().unwrap_or(0);
                self.clock.ends.insert(voice, self.clock.now_cs + len);
            }
            VoiceCommand::Stop { voice } => {
                self.clock.ends.remove(&voice);
            }
            _ => {}
        }
        self.out.push(command);
    }
}

/// Engine bank name of the first `nfs5.car` record with this SIM name.
pub fn bank_for_sim(car_file: &[u8], sim_name: &str) -> Result<String, String> {
    let entries = parse_car_catalogue(car_file)?;
    find_car(&entries, sim_name)
        .map(|e| e.bank_name.clone())
        .ok_or_else(|| format!("в nfs5.car нет автомобиля {sim_name}"))
}

pub struct EngineAudioCore {
    mixer: EngineMixer,
    inputs: EngineSoundInputs,
    clock: StepClock,
    slots: Vec<SlotPcm>,
    accumulator: f32,
}

fn find_by_basename<'a>(files: &'a BTreeMap<String, Vec<u8>>, base: &str) -> Option<&'a [u8]> {
    let want = base.to_ascii_lowercase();
    files
        .iter()
        .find(|(name, _)| {
            name.replace('\\', "/")
                .rsplit('/')
                .next()
                .is_some_and(|b| b.eq_ignore_ascii_case(&want))
        })
        .map(|(_, d)| d.as_slice())
}

impl EngineAudioCore {
    /// `files` must contain `<sim_name>.spl` and `<bank>.viv` (any directory prefix).
    pub fn load(
        files: &BTreeMap<String, Vec<u8>>,
        sim_name: &str,
        bank: &str,
    ) -> Result<Self, String> {
        let spl_bytes = find_by_basename(files, &format!("{sim_name}.spl"))
            .ok_or_else(|| format!("нет файла {sim_name}.spl"))?;
        let viv_bytes = find_by_basename(files, &format!("{bank}.viv"))
            .ok_or_else(|| format!("нет файла {bank}.viv"))?;
        let spl = parse_spl(spl_bytes).map_err(|e| format!("{sim_name}.spl: {e}"))?;
        let tables = engine_tables_from_viv(viv_bytes).map_err(|e| format!("{bank}.viv: {e}"))?;

        let entries = parse_viv(viv_bytes).map_err(|e| format!("{bank}.viv: {e}"))?;
        let bnk_entry = entries
            .iter()
            .rev()
            .find(|e| e.name.to_ascii_lowercase().ends_with(".bnk"))
            .ok_or_else(|| format!("{bank}.viv: нет банка .bnk"))?;
        let bnk_bytes = viv_file(viv_bytes, &entries, &bnk_entry.name)
            .ok_or_else(|| format!("{bank}.viv: нет банка .bnk"))?;
        let parsed = parse_bnk(bnk_bytes).map_err(|e| format!("{}: {e}", bnk_entry.name))?;

        let mut wanted: BTreeSet<u8> = ONE_SHOT_SLOTS.collect();
        for layer in tables.iter().flat_map(|t| t.layers.iter().flatten()) {
            wanted.insert(layer.slot);
        }
        let mut slots = Vec::new();
        let mut clock = StepClock::default();
        for id in wanted {
            let Some(patch) = parsed.patch(id as usize) else {
                continue;
            };
            let pcm = decode_patch(bnk_bytes, patch).map_err(|e| format!("слот {id}: {e}"))?;
            let ch = patch.channels.max(1) as usize;
            let channels = (0..ch)
                .map(|c| {
                    pcm.iter()
                        .skip(c)
                        .step_by(ch)
                        .map(|&s| s as f32 / 32768.0)
                        .collect()
                })
                .collect();
            let loop_range = match (patch.loop_start, patch.loop_end) {
                (Some(s), Some(e)) if s < e => Some((s, e)),
                _ => None,
            };
            if ONE_SHOT_SLOTS.contains(&id) {
                let secs = patch.num_samples as f64 / patch.base_rate_hz().max(1.0);
                clock.length_cs.insert(id, (secs * 100.0).ceil() as i64);
            }
            slots.push(SlotPcm {
                id,
                sample_rate: patch.sample_rate,
                detune_cents: patch.detune_cents(),
                loop_range,
                channels,
            });
        }
        Ok(Self {
            mixer: EngineMixer::new(spl, &tables, MixerGlobals::default()),
            inputs: EngineSoundInputs::new(),
            clock,
            slots,
            accumulator: 0.0,
        })
    }

    pub fn slots(&self) -> &[SlotPcm] {
        &self.slots
    }

    /// Skips (`true`) or replays (`false`) the start sample.
    pub fn set_running(&mut self, running: bool) {
        self.mixer.set_running(running);
    }

    /// Advances by `dt` seconds in fixed 1/100 s mixer steps and returns the voice commands of
    /// all steps, flattened by [`flatten_command`]. `view`: 0 Chase, 1 Bumper, 2 Free.
    pub fn update(
        &mut self,
        dt: f32,
        rpm: f32,
        redline: f32,
        throttle: f32,
        view: u8,
        gear: i32,
    ) -> Vec<i32> {
        self.accumulator += dt.clamp(0.0, 1.0);
        let step = 1.0 / STEP_HZ;
        let telemetry = EngineTelemetry {
            rpm,
            redline_rpm: redline,
            throttle,
            gear,
            view: match view {
                1 => EngineView::Bumper,
                2 => EngineView::Free,
                _ => EngineView::Chase,
            },
        };
        let mut commands = Vec::new();
        let mut steps = 0;
        while self.accumulator >= step && steps < MAX_STEPS_PER_CALL {
            self.accumulator -= step;
            steps += 1;
            self.clock.now_cs += 1;
            let args = self.inputs.step(&telemetry);
            let mut host = Host {
                clock: &mut self.clock,
                out: &mut commands,
            };
            self.mixer.update_with(&args, &mut host);
        }
        if steps == MAX_STEPS_PER_CALL {
            self.accumulator = 0.0;
        }
        commands.iter().flat_map(flatten_command).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game_dir() -> Option<std::path::PathBuf> {
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

    #[test]
    fn flatten_uses_signed_pan_for_loop_voices() {
        let c = VoiceCommand::SetPan {
            voice: VoiceId(3),
            pan: 0xFFFF,
        };
        assert_eq!(flatten_command(&c), [CMD_SET_PAN, 3, 0, 0, 0, -1, 0]);
        let s = VoiceCommand::Start {
            voice: VoiceId(1),
            slot: 7,
            volume: 90,
            pitch: 0x1000,
            pan: -0x8000,
            looped: true,
        };
        assert_eq!(
            flatten_command(&s),
            [CMD_START, 1, 7, 90, 0x1000, -0x8000, 1]
        );
    }

    #[test]
    fn one_shot_clock_counts_steps() {
        let mut clock = StepClock::default();
        clock.length_cs.insert(6, 50);
        clock.now_cs = 100;
        let mut out = Vec::new();
        let mut host = Host {
            clock: &mut clock,
            out: &mut out,
        };
        let v = VoiceId(1);
        assert!(host.is_finished(v), "unknown voice counts as finished");
        host.command(VoiceCommand::Start {
            voice: v,
            slot: 6,
            volume: 1,
            pitch: 0x1000,
            pan: 0,
            looped: false,
        });
        assert!(!host.is_finished(v));
        assert_eq!(host.time_remaining_cs(v), 50);
        host.clock.now_cs = 149;
        assert_eq!(host.time_remaining_cs(v), 1);
        host.clock.now_cs = 150;
        assert!(host.is_finished(v));
    }

    #[test]
    fn bank_lookup_from_installed_catalogue() {
        let Some(dir) = game_dir() else { return };
        let car = std::fs::read(dir.join("../../FEData/Data/nfs5.car")).unwrap();
        assert_eq!(bank_for_sim(&car, "356Acabrio16").unwrap(), "sd1a");
        assert!(bank_for_sim(&car, "nonexistent").is_err());
    }

    #[test]
    fn missing_files_are_reported() {
        let err = EngineAudioCore::load(&BTreeMap::new(), "356Acabrio16", "sd1a")
            .err()
            .unwrap();
        assert!(err.contains("356Acabrio16.spl"), "{err}");
    }

    /// Needs the installed game; skipped when `game/app` isn't found above the cwd.
    #[test]
    fn installed_bank_plays_start_sample_then_loops() {
        let Some(dir) = game_dir() else { return };
        let mut files = BTreeMap::new();
        files.insert(
            "gamedata/sounds/356acabrio16.spl".to_string(),
            std::fs::read(dir.join("356Acabrio16.spl")).unwrap(),
        );
        files.insert(
            "gamedata/sounds/sd1a.viv".to_string(),
            std::fs::read(dir.join("sd1a.viv")).unwrap(),
        );
        let mut core = EngineAudioCore::load(&files, "356Acabrio16", "sd1a").unwrap();
        assert!(!core.slots().is_empty());
        assert!(core.slots().iter().any(|s| s.loop_range.is_some()));
        let mut starts = Vec::new();
        for _ in 0..600 {
            let out = core.update(0.01, 3000.0, 6000.0, 1.0, 1, 2);
            for c in out.chunks(COMMAND_STRIDE) {
                if c[0] == CMD_START {
                    starts.push((c[2], c[6]));
                }
            }
        }
        assert!(
            starts.iter().any(|&(_, looped)| looped == 0),
            "start sample: {starts:?}"
        );
        assert!(
            starts.iter().any(|&(_, looped)| looped == 1),
            "loop voices: {starts:?}"
        );
        for &(slot, _) in &starts {
            assert!(
                core.slots().iter().any(|s| s.id as i32 == slot),
                "slot {slot} decoded"
            );
        }
    }

    #[test]
    fn long_frames_are_capped() {
        let Some(dir) = game_dir() else { return };
        let mut files = BTreeMap::new();
        files.insert(
            "a.spl".to_string(),
            std::fs::read(dir.join("356Acabrio16.spl")).unwrap(),
        );
        files.insert(
            "b.viv".to_string(),
            std::fs::read(dir.join("sd1a.viv")).unwrap(),
        );
        let mut core = EngineAudioCore::load(&files, "a", "b").unwrap();
        core.update(0.5, 3000.0, 6000.0, 0.0, 0, 1);
        assert_eq!(core.clock.now_cs, MAX_STEPS_PER_CALL as i64);
    }
}

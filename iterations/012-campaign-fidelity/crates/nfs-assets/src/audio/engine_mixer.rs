//! Player engine sound mixer, ported from `nfs5.exe`.
//!
//! - `EngineAudio_PushState` `0x4ac780`: RPM bin, pitch, load and group fades, master volume,
//!   start/stop samples and their gain.
//! - `EngineAudio_Mix` `0x4ac290`: 32 voice targets from the CRDl curves and the crossfade
//!   table `0x5d1764`, slewed by at most 8 per update, then the voices are driven.
//! - Throttle smoothing of `CarSound_Update` `0x4aa050`.
//!
//! The mixer calls no audio API. Every audio-library call of the original
//! (`Audio_PlayPatch 0x565e70`, `Audio_StopVoice 0x565d60`, `Audio_SetVolume 0x566340`,
//! `Audio_SetPan 0x5661f0`, `Audio_SetPitch 0x565f80`) becomes a [`VoiceCommand`]; the two
//! queries (`Audio_IsFinished 0x566380`, `Audio_TimeRemaining 0x566430`) go to a
//! [`OneShotClock`]. Voice handles are [`VoiceId`]s allocated by the mixer (0, 1, 2, ...).
//!
//! Integer arithmetic follows the original, including 32-bit wrapping. The original pushes
//! states into a 32-entry ring consumed every third audio-library tick; here every
//! [`EngineMixer::update`] pushes one state and consumes it immediately, which is what the
//! original does when the consumer keeps up. Notes: `docs/re/audio.md` ("Execution results").
use nfs_formats::{parse_crd, parse_viv, CrdTable, EngineSpl, ENGINE_TABLE_EXTS};

/// Number of loop voices: `.ect` 0..7, `.elt` 8..15, `.cct` 16..23, `.clt` 24..31.
pub const ENGINE_VOICES: usize = 32;

/// File offset of [`XFADE_TABLE`] in `nfs5.exe` (VA `0x5d1764`).
pub const XFADE_TABLE_FILE_OFFSET: usize = 0x1d1764;

/// `g_EngineXfadeTable` `0x5d1764`: 129-byte equal-power-like crossfade, `T[0] = 0`,
/// `T[128] = 127`. Copied from `nfs5.exe`.
pub const XFADE_TABLE: [u8; 129] = [
    0, 3, 7, 10, 13, 16, 19, 22, 24, 27, 29, 31, 33, 35, 37, 39, 41, 43, 45, 46, 48, 50, 51, 53,
    54, 55, 57, 58, 60, 61, 62, 63, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80,
    81, 82, 83, 84, 84, 85, 86, 87, 88, 88, 89, 90, 91, 91, 92, 93, 94, 94, 95, 96, 96, 97, 98, 98,
    99, 100, 100, 101, 101, 102, 103, 103, 104, 104, 105, 106, 106, 107, 107, 108, 108, 109, 109,
    110, 110, 111, 111, 112, 112, 113, 113, 114, 114, 115, 115, 116, 116, 117, 117, 118, 118, 119,
    119, 119, 120, 120, 121, 121, 122, 122, 122, 123, 123, 124, 124, 125, 125, 125, 126, 127, 127,
];

/// `g_SfxVolumeCap` `0x5d1748`: volume cap of the start/stop samples.
const SFX_VOLUME_CAP: u16 = 127;
/// Full start/stop gain (`0x4ac780`, `>> 9`).
const FULL_GAIN: i32 = 0x200;
/// Pan used by slots below 0x40 while spread is on (`0x4ac290`).
const PAN_SPREAD_LOW: i16 = -0x8000;

/// Voice handle issued by the mixer. The original uses audio-library handles and `-1` for none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VoiceId(pub u32);

/// One audio-library call of the original, to be executed by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceCommand {
    /// `Audio_PlayPatch 0x565e70(bank, slot, params)`: start a voice on bank `slot`.
    /// `volume` 0..127 (params +8), `pitch` 4.12 with 0x1000 = ×1.0 on top of the sample's own
    /// detune (params +0xC), `pan` ±0x8000 (params +0x18). `looped` is params +0xB: loop
    /// layers 1, start/stop one-shots 0 (they keep the default pitch 0x1000).
    Start {
        voice: VoiceId,
        slot: u8,
        volume: u8,
        pitch: u16,
        pan: i16,
        looped: bool,
    },
    /// `Audio_SetVolume 0x566340(handle, volume)`, 0..127.
    SetVolume { voice: VoiceId, volume: u8 },
    /// `Audio_SetPan 0x5661f0(handle, pan, 0)` with the raw 32-bit argument: the loop voices
    /// pass their 16-bit pan zero-extended (0..0xFFFF, read it as `pan as i16`), the start/stop
    /// samples pass the caller's pan unchanged.
    SetPan { voice: VoiceId, pan: i32 },
    /// `Audio_SetPitch 0x565f80(handle, pitch)`, 4.12.
    SetPitch { voice: VoiceId, pitch: u16 },
    /// `Audio_StopVoice 0x565d60(handle)`.
    Stop { voice: VoiceId },
}

/// Answers the producer's queries about the start/stop one-shots.
pub trait OneShotClock {
    /// `Audio_IsFinished 0x566380`: true when the voice has ended (or is unknown).
    fn is_finished(&mut self, voice: VoiceId) -> bool;
    /// `Audio_TimeRemaining 0x566430`: remaining play time in 1/100 s.
    fn time_remaining_cs(&mut self, voice: VoiceId) -> i32;
}

/// Receives commands in call order, interleaved with the [`OneShotClock`] queries.
pub trait VoiceHost: OneShotClock {
    fn command(&mut self, command: VoiceCommand);
}

/// Arguments of `EngineAudio_PushState 0x4ac780` (after the player index).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PushStateArgs {
    /// Volume argument (`((127 × 20 × 0x55 >> 7) × v) >> 14` in `0x4aa050`).
    pub volume: i32,
    /// RPM over the per-car maximum, 16.16 (0x10000 = redline).
    pub rpm16: i32,
    /// Load: smoothed throttle `>> 1` (0..127), 127 for full load, −1 for engine off.
    pub load: i32,
    /// View mode 0..3 (0 = cockpit tables, 1 = bumper, 2 = tail cams, 3 = other exterior).
    pub mode: i32,
    /// Doppler factor, 16.16 (0x10000 = none).
    pub doppler: i32,
    /// Pan for modes ≥ 2 and for the start/stop samples, ±0x8000.
    pub pan: i32,
    /// Front/back value for modes ≥ 2, ±0x10000.
    pub front_back: i32,
}

impl Default for PushStateArgs {
    fn default() -> Self {
        Self {
            volume: 0,
            rpm16: 0,
            load: 0,
            mode: 3,
            doppler: 0x10000,
            pan: 0,
            front_back: 0,
        }
    }
}

/// Globals the original reads besides the engine-audio state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MixerGlobals {
    /// `g_AudioDetail 0x657c98`: ≥ 2 mutes slots below 0x40 (use the `*16.viv` tables).
    pub detail: i32,
    /// `0x657404 == 1` (split screen): no spread.
    pub split_screen: bool,
    /// `0x655f34`: spread source for modes 0 and 1 (meaning unknown; 0 = no spread).
    pub spread_source: i32,
}

/// One 16-byte ring state as consumed by the mixer (u16 words in ring order).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EngineAudioFrame {
    /// Master volume after the start/stop gain.
    pub master: u16,
    /// Common pitch, 4.12.
    pub pitch: u16,
    pub pan: u16,
    pub spread: u16,
    /// RPM bin 0..511.
    pub bin: u16,
    /// Bank-group crossfade position 0..128.
    pub group: u16,
    /// Load crossfade position 0..128.
    pub load: u16,
    pub mode: u16,
}

/// Throttle smoothing of `CarSound_Update 0x4aa050` (`g_SmoothedThrottle 0x6563c8`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ThrottleSmoother {
    /// Stored smoothed value (the original's per-car slot).
    pub value: i32,
}

impl ThrottleSmoother {
    /// Moves toward the throttle byte: up by `max(gap / 2, 1)`, down by `min(gap / 8, −1)`
    /// (C division), and returns the value capped at 255.
    pub fn step(&mut self, throttle: u8) -> i32 {
        let target = throttle as i32;
        if self.value < target {
            self.value += ((target - self.value) / 2).max(1);
        } else if target < self.value {
            self.value += ((target - self.value) / 8).min(-1);
        }
        self.value.min(0xff)
    }
}

#[derive(Debug, Clone)]
struct VoiceTable {
    /// Bank slot as the original's signed char; negative = unused voice.
    slot: i8,
    start: i16,
    end: i16,
    /// Active part of the volume curve, bins `start..end`.
    curve: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Default)]
struct VoiceCache {
    volume: u8,
    pitch: u16,
    pan: i16,
    voice: Option<VoiceId>,
}

/// Reads the four CRDl tables (`.ect`, `.elt`, `.cct`, `.clt`) of an engine archive
/// (`sd*.viv`). Like `0x4acc10`, entries are matched by their last four characters; the
/// last match wins.
pub fn engine_tables_from_viv(viv: &[u8]) -> Result<[CrdTable; 4], String> {
    let entries = parse_viv(viv)?;
    let table = |ext: &str| -> Result<CrdTable, String> {
        let suffix = format!(".{ext}");
        let e = entries
            .iter()
            .rev()
            .find(|e| e.name.len() >= 4 && e.name[e.name.len() - 4..].eq_ignore_ascii_case(&suffix))
            .ok_or_else(|| format!("engine archive has no {suffix} table"))?;
        parse_crd(&viv[e.offset..e.offset + e.size]).map_err(|err| format!("{suffix}: {err}"))
    };
    Ok([
        table(ENGINE_TABLE_EXTS[0])?,
        table(ENGINE_TABLE_EXTS[1])?,
        table(ENGINE_TABLE_EXTS[2])?,
        table(ENGINE_TABLE_EXTS[3])?,
    ])
}

/// Crossfade weight `T[i]` as a signed char. The index stays in 0..=128 for all shipped
/// `.spl` values with |front_back| ≤ 0x10000; outside that the original reads neighbouring
/// data, here it is clamped.
fn xfade(i: i32) -> i32 {
    XFADE_TABLE[i.clamp(0, 128) as usize] as i8 as i32
}

/// Engine-audio state of one player (`0x656aa8[0]`, 0x758 bytes in the original).
#[derive(Debug, Clone)]
pub struct EngineMixer {
    spl: EngineSpl,
    globals: MixerGlobals,
    voices: Vec<VoiceTable>,
    /// +0x44 `vol[32]`.
    volumes: [i32; ENGINE_VOICES],
    /// +0x454 first-voice cache, +0x5D4 spread-voice cache.
    cache: [VoiceCache; ENGINE_VOICES],
    spread_cache: [VoiceCache; ENGINE_VOICES],
    /// +0x38, +0x3C, +0x40.
    start_voice: Option<VoiceId>,
    stop_voice: Option<VoiceId>,
    running: bool,
    /// +0xC4, +0xC8, +0xCC: pan, spread, pitch of the last consumed state (zero-extended).
    out_pan: u32,
    out_spread: u32,
    out_pitch: u32,
    frame: EngineAudioFrame,
    gain: i32,
    master_before_gain: u16,
    next_voice: u32,
}

impl EngineMixer {
    /// State as `EngineAudio_Load 0x4acc10` leaves it: tables relocated per voice, no voices
    /// playing, not running (the first update with load ≥ 0 plays the start sample).
    pub fn new(spl: EngineSpl, tables: &[CrdTable; 4], globals: MixerGlobals) -> Self {
        let mut voices = vec![
            VoiceTable {
                slot: -1,
                start: 0x200,
                end: 0x200,
                curve: Vec::new(),
            };
            ENGINE_VOICES
        ];
        for (t, table) in tables.iter().enumerate() {
            for (j, layer) in table.layers.iter().enumerate().take(8) {
                let Some(layer) = layer else { continue };
                let range = layer.active_range();
                voices[8 * t + j] = VoiceTable {
                    slot: layer.slot as i8,
                    start: range.start as i16,
                    end: range.end as i16,
                    curve: layer.volume[range].to_vec(),
                };
            }
        }
        Self {
            spl,
            globals,
            voices,
            volumes: [0; ENGINE_VOICES],
            cache: [VoiceCache::default(); ENGINE_VOICES],
            spread_cache: [VoiceCache::default(); ENGINE_VOICES],
            start_voice: None,
            stop_voice: None,
            running: false,
            out_pan: 0,
            out_spread: 0,
            out_pitch: 0x1000,
            frame: EngineAudioFrame::default(),
            gain: 0,
            master_before_gain: 0,
            next_voice: 0,
        }
    }

    /// Sets the "running" flag (+0x40). `true` skips the start sample, as after it has ended.
    pub fn set_running(&mut self, running: bool) {
        self.running = running;
    }

    pub fn set_globals(&mut self, globals: MixerGlobals) {
        self.globals = globals;
    }

    pub fn running(&self) -> bool {
        self.running
    }
    pub fn start_voice(&self) -> Option<VoiceId> {
        self.start_voice
    }
    pub fn stop_voice(&self) -> Option<VoiceId> {
        self.stop_voice
    }
    /// Current voice volumes 0..127 (+0x44).
    pub fn volumes(&self) -> &[i32; ENGINE_VOICES] {
        &self.volumes
    }
    /// Bank slot of each voice, `None` for unused voices.
    pub fn voice_slot(&self, voice: usize) -> Option<u8> {
        let s = self.voices[voice].slot;
        (s >= 0).then_some(s as u8)
    }
    /// Volume curve value at `bin` as the mixer sees it (0 outside the active range).
    pub fn curve_at(&self, voice: usize, bin: u16) -> u8 {
        let v = &self.voices[voice];
        let bin = bin as i32;
        if (v.start as i32..v.end as i32).contains(&bin) {
            v.curve[(bin - v.start as i32) as usize]
        } else {
            0
        }
    }
    /// The last state consumed by the mixer.
    pub fn frame(&self) -> &EngineAudioFrame {
        &self.frame
    }
    /// Start/stop gain of the last update, 0..512 (`0x4acb54`).
    pub fn gain(&self) -> i32 {
        self.gain
    }
    /// Master volume of the last update before the gain was applied.
    pub fn master_before_gain(&self) -> u16 {
        self.master_before_gain
    }
    /// Pan, spread and pitch of the last consumed state as stored at +0xC4/+0xC8/+0xCC.
    pub fn output_pan_spread_pitch(&self) -> (u32, u32, u32) {
        (self.out_pan, self.out_spread, self.out_pitch)
    }

    /// One producer call plus the mixer update that consumes it; returns the voice commands.
    pub fn update(
        &mut self,
        args: &PushStateArgs,
        clock: &mut dyn OneShotClock,
    ) -> Vec<VoiceCommand> {
        struct Collect<'a> {
            clock: &'a mut dyn OneShotClock,
            out: Vec<VoiceCommand>,
        }
        impl OneShotClock for Collect<'_> {
            fn is_finished(&mut self, voice: VoiceId) -> bool {
                self.clock.is_finished(voice)
            }
            fn time_remaining_cs(&mut self, voice: VoiceId) -> i32 {
                self.clock.time_remaining_cs(voice)
            }
        }
        impl VoiceHost for Collect<'_> {
            fn command(&mut self, command: VoiceCommand) {
                self.out.push(command);
            }
        }
        let mut host = Collect {
            clock,
            out: Vec::new(),
        };
        self.update_with(args, &mut host);
        host.out
    }

    /// Like [`Self::update`], sending commands and queries to `host` in the original call order.
    pub fn update_with(&mut self, args: &PushStateArgs, host: &mut dyn VoiceHost) {
        self.push_state(args, host);
        self.mix(host);
    }

    fn alloc_voice(&mut self) -> VoiceId {
        let id = VoiceId(self.next_voice);
        self.next_voice = self.next_voice.wrapping_add(1);
        id
    }

    /// `EngineAudio_PushState 0x4ac780`.
    fn push_state(&mut self, a: &PushStateArgs, host: &mut dyn VoiceHost) {
        let spl = self.spl.0;
        let mut f = EngineAudioFrame::default();

        let product = spl[9].wrapping_mul(a.rpm16);
        f.bin = if ((product as u32 & 0xffff_c000) as i32) < 0x7f_c000 {
            (product >> 14) as u16
        } else {
            0x1ff
        };

        // 64-bit product, bits 16..47 plus the rounding bit 15, then a 32-bit scale.
        let wide = a.rpm16.wrapping_add(0x3333) as i64 * a.doppler as i64;
        let scaled = ((wide >> 16) as i32).wrapping_add(((wide >> 15) & 1) as i32);
        f.pitch = (spl[8].wrapping_mul(scaled) >> 10) as u16;

        let load = a
            .load
            .wrapping_add(a.load >> 6)
            .wrapping_add(a.load >> 5)
            .clamp(0, 0x80);
        f.load = load as u16;
        f.mode = a.mode as u16;

        let master_field = if a.mode < 2 {
            f.group = if a.mode == 0 { spl[3] } else { spl[1] } as u16;
            f.pan = 0;
            f.spread = if self.globals.split_screen || self.globals.detail > 1 {
                0
            } else {
                (self.globals.spread_source.wrapping_mul(0x3fff) >> 7) as u16
            };
            if a.mode == 0 {
                spl[2]
            } else {
                spl[0]
            }
        } else {
            let base = spl[4] as u16;
            let w = if a.front_back < 0 {
                spl[5].wrapping_mul(base as i32)
            } else {
                (0x80u32.wrapping_sub(base as u32) as i32).wrapping_mul(spl[6])
            };
            f.spread = 0;
            f.pan = a.pan as u16;
            // Signed division by 65536 (rounding toward zero) of a wrapping 32-bit product.
            let x = w.wrapping_mul(a.front_back);
            let q = (x >> 7).wrapping_add((x >> 31) & 0xffff) >> 16;
            f.group = (q as i16 as u16).wrapping_add(base);
            spl[7]
        };
        let m = master_field.wrapping_mul(a.volume) >> 6;
        let master = if m > 0x7fe { 0x7ff } else { m as u16 };
        self.master_before_gain = master;

        let oneshot_volume = master.min(SFX_VOLUME_CAP) as u8;
        let oneshot_slot = |base: i32| (a.mode.min(2) + base) as u8;
        let gain = 'gain: {
            if a.load < 0 {
                if let Some(v) = self.start_voice.take() {
                    host.command(VoiceCommand::Stop { voice: v });
                    self.running = true;
                }
                if self.running {
                    match self.stop_voice {
                        None => {
                            let voice = self.alloc_voice();
                            host.command(VoiceCommand::Start {
                                voice,
                                slot: oneshot_slot(9),
                                volume: oneshot_volume,
                                pitch: 0x1000,
                                pan: a.pan as i16,
                                looped: false,
                            });
                            self.stop_voice = Some(voice);
                        }
                        Some(voice) => {
                            if host.is_finished(voice) {
                                self.running = false;
                                self.stop_voice = None;
                            } else {
                                host.command(VoiceCommand::SetVolume {
                                    voice,
                                    volume: oneshot_volume,
                                });
                                host.command(VoiceCommand::SetPan { voice, pan: a.pan });
                            }
                        }
                    }
                }
                0
            } else {
                if let Some(v) = self.stop_voice.take() {
                    host.command(VoiceCommand::Stop { voice: v });
                    self.running = false;
                }
                if self.running {
                    break 'gain FULL_GAIN;
                }
                match self.start_voice {
                    None => {
                        let voice = self.alloc_voice();
                        host.command(VoiceCommand::Start {
                            voice,
                            slot: oneshot_slot(6),
                            volume: oneshot_volume,
                            pitch: 0x1000,
                            pan: a.pan as i16,
                            looped: false,
                        });
                        self.start_voice = Some(voice);
                        0
                    }
                    Some(voice) => {
                        if host.is_finished(voice) {
                            self.running = true;
                            self.start_voice = None;
                            break 'gain FULL_GAIN;
                        }
                        host.command(VoiceCommand::SetVolume {
                            voice,
                            volume: oneshot_volume,
                        });
                        host.command(VoiceCommand::SetPan { voice, pan: a.pan });
                        // Read twice, as the original does.
                        if FULL_GAIN.wrapping_sub(host.time_remaining_cs(voice)) >= 0 {
                            FULL_GAIN.wrapping_sub(host.time_remaining_cs(voice))
                        } else {
                            0
                        }
                    }
                }
            }
        };
        self.gain = gain;
        f.master = ((master as u32).wrapping_mul(gain as u32) as i32 >> 9) as u16;
        self.frame = f;
    }

    /// `EngineAudio_Mix 0x4ac290`, consuming [`Self::frame`].
    fn mix(&mut self, host: &mut dyn VoiceHost) {
        let f = self.frame;
        let detail = self.globals.detail;
        for i in 0..ENGINE_VOICES {
            let v = &self.voices[i];
            if v.slot < 0 {
                continue;
            }
            let mut target = 0i32;
            let in_group = (f.mode != 0 && i < 16) || (f.mode == 0 && i > 15);
            let bin = f.bin as i32;
            if in_group && v.start as i32 <= bin && bin < v.end as i32 {
                let c = v.curve[(bin - v.start as i32) as usize] as i8 as i32;
                if c != 0 {
                    let no_load_set = i < 8 || (16..24).contains(&i);
                    let load_w = if no_load_set {
                        xfade(128 - f.load as i32)
                    } else {
                        xfade(f.load as i32)
                    };
                    let c = load_w.wrapping_mul(c);
                    target = if v.slot < 0x40 {
                        if detail < 2 {
                            xfade(f.group as i32).wrapping_mul(c)
                        } else {
                            0
                        }
                    } else if detail < 2 {
                        let g = xfade(128 - f.group as i32);
                        if f.spread == 0 {
                            g.wrapping_mul(c)
                        } else {
                            ((g * 0x5e) >> 7).wrapping_mul(c)
                        }
                    } else {
                        c.wrapping_mul(0x7f)
                    };
                }
            }
            let target =
                (((target >> 3) as u32).wrapping_mul(f.master as u32) as i32 >> 18).min(0x7f);
            let vol = &mut self.volumes[i];
            *vol = if *vol + 8 < target {
                *vol + 8
            } else if target < *vol - 8 {
                *vol - 8
            } else {
                target
            };
        }

        self.out_pan = f.pan as u32;
        self.out_spread = f.spread as u32;
        self.out_pitch = f.pitch as u32;
        let pan = self.out_pan as u16 as i16;
        let spread = self.out_spread as i32;
        let pitch = self.out_pitch as u16;

        for i in 0..ENGINE_VOICES {
            let slot = self.voices[i].slot;
            if slot < 0 {
                continue;
            }
            let mut vol = self.volumes[i];
            if vol == 0 {
                if let Some(voice) = self.cache[i].voice {
                    host.command(VoiceCommand::Stop { voice });
                    if let Some(voice) = self.spread_cache[i].voice {
                        host.command(VoiceCommand::Stop { voice });
                    }
                    self.cache[i].voice = None;
                    self.spread_cache[i].voice = None;
                }
                continue;
            }
            let (pan1, pan2) = if slot < 0x40 {
                if spread < 1 {
                    if self.cache[i].pan == PAN_SPREAD_LOW && (self.cache[i].volume as i8) > 0 {
                        // Leaving spread: one update at volume 0, pan unchanged.
                        vol = 0;
                        (PAN_SPREAD_LOW, PAN_SPREAD_LOW)
                    } else {
                        (pan, pan)
                    }
                } else {
                    (PAN_SPREAD_LOW, PAN_SPREAD_LOW)
                }
            } else {
                let s = spread as i16;
                (pan.wrapping_sub(s), s.wrapping_add(pan))
            };
            let volume = vol as u8;
            let slot = slot as u8;
            let Some(first) = self.cache[i].voice else {
                let voice = self.alloc_voice();
                host.command(VoiceCommand::Start {
                    voice,
                    slot,
                    volume,
                    pitch,
                    pan: pan1,
                    looped: true,
                });
                self.cache[i] = VoiceCache {
                    volume,
                    pitch,
                    pan: pan1,
                    voice: Some(voice),
                };
                continue;
            };
            Self::refresh(&mut self.cache[i], first, vol, pan1, self.out_pitch, host);
            if slot < 0x40 || spread == 0 {
                if let Some(voice) = self.spread_cache[i].voice.take() {
                    host.command(VoiceCommand::Stop { voice });
                }
            } else if let Some(second) = self.spread_cache[i].voice {
                Self::refresh(
                    &mut self.spread_cache[i],
                    second,
                    vol,
                    pan2,
                    self.out_pitch,
                    host,
                );
            } else {
                let voice = self.alloc_voice();
                host.command(VoiceCommand::Start {
                    voice,
                    slot,
                    volume,
                    pitch,
                    pan: pan2,
                    looped: true,
                });
                self.spread_cache[i] = VoiceCache {
                    volume,
                    pitch,
                    pan: pan2,
                    voice: Some(voice),
                };
            }
        }
    }

    /// Sends volume, pan and pitch of a playing voice when they differ from its cache.
    fn refresh(
        cache: &mut VoiceCache,
        voice: VoiceId,
        vol: i32,
        pan: i16,
        pitch: u32,
        host: &mut dyn VoiceHost,
    ) {
        if cache.volume as i8 as i32 != vol {
            host.command(VoiceCommand::SetVolume {
                voice,
                volume: vol as u8,
            });
            cache.volume = vol as u8;
        }
        if cache.pan != pan {
            // Pushed zero-extended (`and edx, 0xffff` before 0x4ac5e4 / 0x4ac6c7).
            host.command(VoiceCommand::SetPan {
                voice,
                pan: pan as u16 as i32,
            });
            cache.pan = pan;
        }
        if cache.pitch as u32 != pitch {
            host.command(VoiceCommand::SetPitch {
                voice,
                pitch: pitch as u16,
            });
            cache.pitch = pitch as u16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xfade_table_matches_documented_points() {
        assert_eq!(
            [0, 32, 64, 96, 128].map(|i| XFADE_TABLE[i]),
            [0, 65, 94, 112, 127]
        );
    }

    #[test]
    fn throttle_smoothing_steps() {
        let mut s = ThrottleSmoother::default();
        assert_eq!(s.step(255), 127);
        assert_eq!(s.step(255), 191);
        assert_eq!(s.step(1), 168); // (1 - 191) / 8 = -23
        let mut s = ThrottleSmoother { value: 5 };
        assert_eq!(s.step(0), 4); // 0 / 8 -> -1
        assert_eq!(s.step(5), 5);
        assert_eq!(s.step(6), 6); // 1 / 2 -> 1
    }
}

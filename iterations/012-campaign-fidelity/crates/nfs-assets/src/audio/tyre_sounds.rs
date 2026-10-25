//! Tyre squeal and road-surface rumble of the player car (`docs/re/audio.md`, "Tyre and surface
//! sounds"). All samples are looped patches of `SND_coll.bnk` (bank group 3).
//!
//! Original chain, per 32 Hz effects tick of the car:
//! - `Car_SkidSoundEvents` 0x40fbc0 (from 0x40fff0) turns the wheel skids `w+0xb8` into queue
//!   events: id 0x12 (front pair, type 2 update / 3 stop) and id 0x14 (rear pair, type 4 / 5,
//!   half volume), kept alive by two bits of `car+0xe00`.
//! - `CarSound_UpdateCar` 0x4a7660 hands each event to `Sfx_PlayEvent` 0x4a93c0, which picks the
//!   squeal slot 0x28..0x2f from the surface patch and the tyre type, scales the volume and
//!   plays it through `Sfx_Play` 0x4a9c10.
//! - `CarSound_Update` 0x4aa050 plays the surface rumble (effect 0x18, slot `0x11 + surface`).
//!
//! Not ported (registered in `docs/approximations.md`): the audio-library output stage, the
//! damaged-car scrub layer, the roof scrape, the listener offset (distance 0, doppler 1, pan 0).
use super::inputs::EngineView;

/// Rate of `Car_ContactEffectsTick` 0x410280 (even scheduler queue, 32 Hz).
pub const TICK_HZ: f32 = 32.0;
/// Fade ticks of `Audio_StopVoiceFade(handle, 5, -1)` (0x4a9890, 0x4a93c0, 0x4aa050).
pub const FADE_STOP_TICKS: u8 = 5;
/// Fade ticks when `Sfx_Play` replaces the slot of a playing voice (0x4a9c10).
pub const FADE_SWITCH_TICKS: u8 = 0x19;
/// Effect level `0x5d1744` and the per-effect constants `0x5d14cc`/`0x5d14d4` (all 127).
pub const SFX_LEVEL: i32 = 127;
pub const SFX_EFFECT_SCALE: i32 = 127;
/// `PlayParams.pitch` of the squeal and of the rumble without doppler (4.12, 1.0).
pub const PITCH_ONE: u16 = 0x1000;
/// First rumble slot: `0x11 + surface class (car+0x464)`.
pub const RUMBLE_FIRST_SLOT: u8 = 0x11;
/// Patch code per surface class, `g_SurfaceSoundPatch` 0x5cc4d8.
pub const SURFACE_PATCH: [i32; 16] = [10, 5, 7, 12, 13, 12, 6, 9, 11, 10, 5, 8, 7, 7, 7, 10];
/// All slots the three channels can play: rumble `0x11..=0x20` and squeal `0x28..=0x2f`.
pub fn tyre_slots() -> impl Iterator<Item = u8> {
    (0x11..=0x20).chain(0x28..=0x2f)
}

/// The three voices (effect ids 0x12, 0x14, 0x18).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// Front tyres (effect id 0x12).
    Front = 0,
    /// Rear tyres (effect id 0x14).
    Rear = 1,
    /// Surface rumble (effect id 0x18).
    Rumble = 2,
}

impl Channel {
    pub const ALL: [Channel; 3] = [Channel::Front, Channel::Rear, Channel::Rumble];
    /// Effect id of the original (`g_SfxHandles` index).
    pub fn effect_id(self) -> u8 {
        match self {
            Channel::Front => 0x12,
            Channel::Rear => 0x14,
            Channel::Rumble => 0x18,
        }
    }
}

/// Car state read by the tyre sounds (original field names in the comments).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TyreInputs {
    /// Skid `w+0xb8` of the front wheels (`car+0x870`, `+0x934`).
    pub skid_front: [f32; 2],
    /// Skid of the rear wheels (`car+0x9f8`, `+0xabc`).
    pub skid_rear: [f32; 2],
    /// `car+0x78` (gate: silent from 60.0; not found in our car, kept 0).
    pub field_78: f32,
    /// Height above the ground `car+0x424`.
    pub height_424: f32,
    /// Ground normal · up `car+0x3a0`.
    pub ground_up_3a0: f32,
    /// Surface class `car+0x464`.
    pub surface_464: i32,
    /// `car+0x540 != 0` plays the rumble of surface 4.
    pub surface_override_540: bool,
    /// `sim+0x1c0` tyre type (0, 1, 2).
    pub tyre_type_1c0: i32,
    /// `car+0x52c` bit 4.
    pub flags_52c_bit4: bool,
    /// Forward speed in the ground frame `car+0xd60` (m/s).
    pub vel_fwd_d60: f32,
    /// Engine rpm `car+0xdb4` and redline `spec+0x94`.
    pub rpm_db4: f32,
    pub redline_94: f32,
    /// `car+0x418 > 5` (wheels off the ground).
    pub wheels_off_gt5: bool,
    /// `res+0xc == 1` (adds 1000 instead of 2000 to the redline in the rpm term).
    pub res_c_is_1: bool,
}

impl Default for TyreInputs {
    fn default() -> Self {
        Self {
            skid_front: [0.0; 2],
            skid_rear: [0.0; 2],
            field_78: 0.0,
            height_424: 0.0,
            ground_up_3a0: 1.0,
            surface_464: 1,
            surface_override_540: false,
            tyre_type_1c0: 0,
            flags_52c_bit4: false,
            vel_fwd_d60: 0.0,
            rpm_db4: 0.0,
            redline_94: 7000.0,
            wheels_off_gt5: false,
            res_c_is_1: false,
        }
    }
}

/// Voice command for the host; `volume` is the final voice volume 0..127 (after the
/// `127·127·v·0x55 >> 21` scale of `Sfx_Play`), `control` the pitch controller 0..127
/// (`Audio_SetVoiceControl` 0x5660c0), `pitch` the 4.12 pitch multiplier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TyreCommand {
    Start {
        voice: u32,
        channel: Channel,
        slot: u8,
        volume: u8,
        control: u8,
        pitch: u16,
    },
    Update {
        voice: u32,
        channel: Channel,
        slot: u8,
        volume: u8,
        control: u8,
        pitch: u16,
    },
    Stop {
        voice: u32,
        fade_ticks: u8,
    },
}

/// A playing voice, for the debug readout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveTyreVoice {
    pub voice: u32,
    pub channel: Channel,
    pub slot: u8,
    pub volume: u8,
    pub control: u8,
    /// 4.12 pitch multiplier.
    pub pitch: u16,
}

/// One queued sound event of `0x40f020` (`car+0x110c`, the fields this chain uses).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkidEvent {
    /// 2 front update, 3 front stop, 4 rear update, 5 rear stop.
    pub kind: i32,
    pub channel: Channel,
    /// Tyre code: 0 / 2 / 1 from `sim+0x1c0` 0 / 1 / 2 (3 = body scrape).
    pub tyre: i32,
    /// Surface patch `0x5cc4d8[surface]`.
    pub patch: i32,
    pub volume: f32,
}

/// Tyre code of the events: `sim+0x1c0` 0 → 0, 1 → 2, 2 → 1, else 0 [VA 0x40fbc0].
pub fn tyre_code(tyre_type_1c0: i32) -> i32 {
    match tyre_type_1c0 {
        1 => 2,
        2 => 1,
        _ => 0,
    }
}

/// Squeal slot by surface patch and tyre code [VA 0x4a93c0].
pub fn squeal_slot(patch: i32, tyre: i32) -> u8 {
    match patch {
        5 | 8 | 9 | 13 => match tyre {
            3 => 0x28,
            1 => 0x2e,
            2 => 0x2f,
            _ => 0x2a,
        },
        6 => {
            if tyre == 3 {
                0x28
            } else {
                0x2d
            }
        }
        7 => 0x29,
        10 | 11 => 0x2b,
        _ => 0x2c,
    }
}

/// `Math_FixedDiv` 0x4a6160: `a / b` in 16.16.
pub fn fixed_div(a: i32, b: i32) -> i32 {
    let rem = a % b;
    let frac = (((rem as i64) << 16) / b as i64) as i32;
    frac.wrapping_add((a / b).wrapping_mul(0x10000))
}

/// Listener-at-car distance attenuation `(fixed_div(num, dist + 0x20000) + rounding) >> 16`
/// (`dist` in 16.16 metres, 0 for the player's own car).
pub fn attenuation(num: i32, dist: i32) -> i32 {
    let x = fixed_div(num, dist.wrapping_add(0x20000));
    x.wrapping_add((x >> 31) & 0xffff) >> 16
}

/// `ftol` of an x87 product, truncating toward zero.
fn ftol(x: f64) -> i32 {
    x as i32
}

/// Engine term of the rumble, argument 3 of `CarSound_Update`: `ftol(rpm · 127 / (redline + K −
/// 500·[wheels off]))`, `K` 1000 when `res+0xc == 1`, else 2000 [VA 0x4a79b7..0x4a7a0c].
pub fn rpm_term(car: &TyreInputs) -> i32 {
    let rev = (car.redline_94 as f64) + if car.res_c_is_1 { 1000.0 } else { 2000.0 }
        - if car.wheels_off_gt5 { 500.0 } else { 0.0 };
    ftol(car.rpm_db4 as f64 * 127.0 / rev)
}

/// Skid-sound state of the player's car: the two bits of `car+0xe00` and the playing voices.
#[derive(Debug, Clone, Default)]
pub struct TyreSounds {
    bits_e00: u8,
    voices: [Option<ActiveTyreVoice>; 3],
    next_voice: u32,
}

impl TyreSounds {
    pub fn new() -> Self {
        Self::default()
    }

    /// Playing voices.
    pub fn active(&self) -> Vec<ActiveTyreVoice> {
        self.voices.iter().flatten().copied().collect()
    }

    /// The voice of one channel.
    pub fn voice(&self, channel: Channel) -> Option<ActiveTyreVoice> {
        self.voices[channel as usize]
    }

    /// `car+0xe00` low bits (bit 0 front, bit 1 rear).
    pub fn bits_e00(&self) -> u8 {
        self.bits_e00
    }

    /// `Car_SkidSoundEvents` 0x40fbc0 followed by the 0x40fff0 stop of the body scrape: the
    /// events the car queues in one effects tick.
    pub fn skid_events(&mut self, car: &TyreInputs) -> Vec<SkidEvent> {
        let mut events = Vec::new();
        let sum_front = car.skid_front[1] + car.skid_front[0];
        let mut rear = car.skid_rear[1] + car.skid_rear[0];
        let front;
        if car.field_78 >= 60.0 || car.height_424 >= 0.1 || car.ground_up_3a0 <= 0.5 {
            front = 0.0;
            rear = 0.0;
        } else {
            front = if sum_front <= 0.0 {
                0.0
            } else {
                sum_front.min(10.0)
            };
            rear = if rear > 0.0 { rear.min(10.0) } else { 0.0 };
        }
        let tyre = tyre_code(car.tyre_type_1c0);
        let patch = SURFACE_PATCH[(car.surface_464 & 0xf) as usize];
        let mut push = |kind, channel, volume| {
            if events.len() < 7 {
                events.push(SkidEvent {
                    kind,
                    channel,
                    tyre,
                    patch,
                    volume,
                });
            }
        };
        if front == 0.0 {
            if self.bits_e00 & 1 != 0 {
                push(3, Channel::Front, 0.0);
                self.bits_e00 -= 1;
            }
        } else if front > 0.0 || car.flags_52c_bit4 {
            push(2, Channel::Front, front);
            self.bits_e00 |= 1;
        }
        if rear == 0.0 {
            if self.bits_e00 & 2 != 0 {
                push(5, Channel::Rear, 0.0);
                self.bits_e00 -= 2;
            }
        } else if rear > 0.0 || car.flags_52c_bit4 {
            push(4, Channel::Rear, rear * 0.5);
            self.bits_e00 |= 2;
        }
        events
    }

    /// One audio update of the car: the skid events, then the surface rumble.
    pub fn tick(&mut self, car: &TyreInputs, view: EngineView) -> Vec<TyreCommand> {
        let mut out = Vec::new();
        for event in self.skid_events(car) {
            self.play_event(&event, &mut out);
        }
        self.rumble(car, view, &mut out);
        out
    }

    /// `Sfx_PlayEvent` 0x4a93c0 for the front and rear events, listener at the car.
    pub fn play_event(&mut self, event: &SkidEvent, out: &mut Vec<TyreCommand>) {
        if event.kind == 3 || event.kind == 5 {
            self.stop(event.channel, FADE_STOP_TICKS, out);
            return;
        }
        let dist = attenuation(0x200_0000, 0);
        let p4 = ftol(event.volume as f64 * 65535.0);
        if p4 <= 0 {
            self.stop(event.channel, FADE_STOP_TICKS, out);
            return;
        }
        let slot = squeal_slot(event.patch, event.tyre);
        let control = (p4.wrapping_mul(0x7f) / 0xa0000).min(0x7f);
        let loud = {
            let x = p4.wrapping_mul(0x7f);
            (x.wrapping_add((x >> 31) & 0xffff) >> 16).min(0x7f)
        };
        let volume = (dist * loud >> 7).min(0x7f);
        self.sfx_play(event.channel, slot, control, PITCH_ONE, volume, out);
    }

    /// Rumble of `CarSound_Update` 0x4aa050 (effect 0x18).
    fn rumble(&mut self, car: &TyreInputs, view: EngineView, out: &mut Vec<TyreCommand>) {
        let class = car.surface_464;
        let mult = if class == 1 { 1.0 } else { 3.0 };
        let speed = ftol((car.vel_fwd_d60 as f64 * mult).abs());
        let half = speed >> 1;
        let control = (half + 0x40).min(0x7f);
        let used = match view {
            EngineView::Bumper => speed,
            EngineView::Chase | EngineView::Free => half,
        };
        let p = rpm_term(car).clamp(0, 0x96);
        let d = attenuation(0x800_0000, 0);
        let x = (p + 0x28).wrapping_mul(d);
        let scaled = x.wrapping_add((x >> 31) & 0x7f) >> 7;
        let reach = scaled.max(d);
        let excess = (used - 15).max(0);
        let volume = ((reach.wrapping_mul(excess)) >> 7).min(0x7a);
        if volume == 0 {
            self.stop(Channel::Rumble, FADE_STOP_TICKS, out);
            return;
        }
        let surface = if car.surface_override_540 { 4 } else { class };
        let slot = (surface + RUMBLE_FIRST_SLOT as i32) as u8;
        self.sfx_play(Channel::Rumble, slot, control, PITCH_ONE, volume, out);
    }

    /// `Sfx_Play` 0x4a9c10: start the slot, update the playing voice, or fade the old one out
    /// when the slot changes.
    fn sfx_play(
        &mut self,
        channel: Channel,
        slot: u8,
        control: i32,
        pitch: u16,
        volume: i32,
        out: &mut Vec<TyreCommand>,
    ) {
        let control = control.clamp(0, 0x7f) as u8;
        let raw = volume.clamp(0, 0x7f);
        let volume = ((SFX_EFFECT_SCALE * SFX_LEVEL * raw * 0x55) >> 21) as u8;
        let i = channel as usize;
        if let Some(playing) = self.voices[i] {
            if playing.slot == slot {
                let changed = playing.volume != volume
                    || playing.control != control
                    || playing.pitch != pitch;
                self.voices[i] = Some(ActiveTyreVoice {
                    volume,
                    control,
                    pitch,
                    ..playing
                });
                if changed {
                    out.push(TyreCommand::Update {
                        voice: playing.voice,
                        channel,
                        slot,
                        volume,
                        control,
                        pitch,
                    });
                }
                return;
            }
            out.push(TyreCommand::Stop {
                voice: playing.voice,
                fade_ticks: FADE_SWITCH_TICKS,
            });
        }
        self.next_voice += 1;
        let voice = self.next_voice;
        self.voices[i] = Some(ActiveTyreVoice {
            voice,
            channel,
            slot,
            volume,
            control,
            pitch,
        });
        out.push(TyreCommand::Start {
            voice,
            channel,
            slot,
            volume,
            control,
            pitch,
        });
    }

    /// `Audio_StopVoiceFade(handle, ticks, -1)`.
    fn stop(&mut self, channel: Channel, fade_ticks: u8, out: &mut Vec<TyreCommand>) {
        if let Some(playing) = self.voices[channel as usize].take() {
            out.push(TyreCommand::Stop {
                voice: playing.voice,
                fade_ticks,
            });
        }
    }

    /// Stops every voice (leaving the race, view without sound).
    pub fn silence(&mut self) -> Vec<TyreCommand> {
        let mut out = Vec::new();
        for channel in Channel::ALL {
            self.stop(channel, FADE_STOP_TICKS, &mut out);
        }
        self.bits_e00 = 0;
        out
    }
}

/// Pitch multiplier (4.12) of the voice controller: `Voice_UpdatePitch` 0x589f40 adds
/// `(control − 0x40) · range·100 >> 6` cents, `range` = patch tag 0x0a.
pub fn control_cents(control: u8, range_semitones: i32) -> i32 {
    ((control as i32 - 0x40) * range_semitones * 100) >> 6
}

#[cfg(test)]
mod tests {
    use super::*;

    fn driving() -> TyreInputs {
        TyreInputs {
            vel_fwd_d60: 30.0,
            rpm_db4: 4000.0,
            ..Default::default()
        }
    }

    #[test]
    fn listener_at_car_attenuation() {
        assert_eq!(attenuation(0x800_0000, 0), 1024);
        assert_eq!(attenuation(0x200_0000, 0), 256);
        assert_eq!(fixed_div(0x800_0000, 0x20000), 0x400_0000);
    }

    #[test]
    fn squeal_slot_table() {
        // Road (surface 1, patch 5): tyre type 0 / 1 / 2 -> 0x2a / 0x2f / 0x2e.
        assert_eq!(squeal_slot(SURFACE_PATCH[1], tyre_code(0)), 0x2a);
        assert_eq!(squeal_slot(SURFACE_PATCH[1], tyre_code(1)), 0x2f);
        assert_eq!(squeal_slot(SURFACE_PATCH[1], tyre_code(2)), 0x2e);
        assert_eq!(squeal_slot(6, 0), 0x2d);
        assert_eq!(squeal_slot(7, 0), 0x29);
        assert_eq!(squeal_slot(10, 0), 0x2b);
        assert_eq!(squeal_slot(12, 0), 0x2c);
        assert_eq!(squeal_slot(5, 3), 0x28);
    }

    #[test]
    fn squeal_starts_with_slip_and_stops_without() {
        let mut t = TyreSounds::new();
        let mut car = driving();
        assert!(t.tick(&car, EngineView::Chase).iter().all(|c| !matches!(
            c,
            TyreCommand::Start {
                channel: Channel::Front | Channel::Rear,
                ..
            }
        )));
        car.skid_front = [0.2, 0.2];
        let cmds = t.tick(&car, EngineView::Chase);
        let start = cmds
            .iter()
            .find(|c| {
                matches!(
                    c,
                    TyreCommand::Start {
                        channel: Channel::Front,
                        ..
                    }
                )
            })
            .expect("front squeal starts");
        // Sum 0.4 -> loud = 50, dist 256 -> volume 100 -> *0.654 = 65.
        let TyreCommand::Start {
            slot,
            volume,
            control,
            ..
        } = *start
        else {
            unreachable!()
        };
        assert_eq!(slot, 0x2a);
        assert_eq!(volume, 65);
        assert_eq!(control, 5);
        assert_eq!(t.bits_e00(), 1);
        car.skid_front = [0.0; 2];
        let cmds = t.tick(&car, EngineView::Chase);
        assert!(cmds.contains(&TyreCommand::Stop {
            voice: 1,
            fade_ticks: FADE_STOP_TICKS
        }));
        assert_eq!(t.bits_e00(), 0);
    }

    #[test]
    fn rear_is_half_volume_and_airborne_is_silent() {
        let mut t = TyreSounds::new();
        let mut car = driving();
        car.skid_rear = [0.5, 0.5];
        let ev = t.skid_events(&car);
        assert_eq!(ev.len(), 1);
        assert_eq!(
            (ev[0].kind, ev[0].channel, ev[0].volume),
            (4, Channel::Rear, 0.5)
        );
        car.height_424 = 0.5;
        let ev = t.skid_events(&car);
        assert_eq!((ev[0].kind, ev[0].channel), (5, Channel::Rear));
        assert_eq!(t.bits_e00(), 0);
    }

    #[test]
    fn rumble_only_on_rough_surfaces_at_low_speed() {
        let mut t = TyreSounds::new();
        let mut car = driving();
        car.vel_fwd_d60 = 8.0;
        // Tarmac (surface 1): speed term 8 -> 4 -> below 15: silent.
        assert!(t.tick(&car, EngineView::Chase).is_empty());
        // Another surface multiplies the speed by 3: 24 -> 12 (chase) is still below 15.
        car.surface_464 = 4;
        assert!(t.tick(&car, EngineView::Chase).is_empty());
        car.vel_fwd_d60 = 15.0;
        let cmds = t.tick(&car, EngineView::Chase);
        let TyreCommand::Start {
            slot,
            volume,
            channel,
            ..
        } = cmds[0]
        else {
            panic!()
        };
        assert_eq!((channel, slot), (Channel::Rumble, 0x15));
        // used = 22 - 15 = 7 -> 1024 * 7 >> 7 = 56 -> 56 * 0.654 = 36.
        assert_eq!(volume, 36);
        // Cockpit uses the full speed term: saturates at 0x7a.
        let mut t = TyreSounds::new();
        let TyreCommand::Start { volume, .. } = t.tick(&car, EngineView::Bumper)[0] else {
            panic!()
        };
        assert_eq!(volume, 79);
    }

    #[test]
    fn surface_change_fades_the_old_voice() {
        let mut t = TyreSounds::new();
        let mut car = driving();
        car.surface_464 = 4;
        car.vel_fwd_d60 = 20.0;
        t.tick(&car, EngineView::Chase);
        car.surface_464 = 6;
        let cmds = t.tick(&car, EngineView::Chase);
        assert_eq!(
            cmds[0],
            TyreCommand::Stop {
                voice: 1,
                fade_ticks: FADE_SWITCH_TICKS
            }
        );
        assert!(matches!(
            cmds[1],
            TyreCommand::Start {
                voice: 2,
                slot: 0x17,
                ..
            }
        ));
    }

    #[test]
    fn control_pitch() {
        assert_eq!(control_cents(0x40, 12), 0);
        assert_eq!(control_cents(0x7f, 12), (63 * 1200) >> 6);
        assert_eq!(control_cents(0, 5), -500);
    }
}

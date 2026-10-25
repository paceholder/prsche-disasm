//! Original NFS5 audio logic without an audio backend.
//!
//! Research notes: `docs/re/audio.md` in the research workspace.
pub mod engine_mixer;
pub mod inputs;
pub mod music;
pub mod speech;
pub mod tyre_sounds;

pub use engine_mixer::{
    engine_tables_from_viv, EngineAudioFrame, EngineMixer, MixerGlobals, OneShotClock,
    PushStateArgs, ThrottleSmoother, VoiceCommand, VoiceHost, VoiceId, ENGINE_VOICES, XFADE_TABLE,
    XFADE_TABLE_FILE_OFFSET,
};
pub use inputs::{
    gear_change_cut, gear_change_volume, rpm16, throttle_byte, volume_argument, EngineSoundInputs,
    EngineTelemetry, EngineView, GEAR_CHANGE_STEPS, LISTENER_VOLUME, UPDATE_HZ,
};
pub use music::{
    glob_match, music_volume, MusicPlayer, StreamCommand, StreamStatus, SwitchState, NONE_FADE_MS,
    SCREEN_FADE_MS,
};
pub use speech::{
    on_lap_completed, on_race_finished, result_music, speech_volume, Countdown, LapEvent,
    SpeechQueue, COUNTDOWN_STEPS, SCHED_HZ, START_SECS, START_STEP,
};
pub use tyre_sounds::{
    attenuation, control_cents, rpm_term, squeal_slot, tyre_code, tyre_slots, ActiveTyreVoice, Channel,
    SkidEvent, TyreCommand, TyreInputs, TyreSounds, FADE_STOP_TICKS, FADE_SWITCH_TICKS,
    RUMBLE_FIRST_SLOT, SURFACE_PATCH, TICK_HZ as TYRE_TICK_HZ,
};

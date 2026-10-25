//! Browser-facing music and race-speech cores (no wasm types, so they test natively).
//!
//! Original logic lives in `nfs_assets::audio::{music, speech}`; formats in
//! `nfs_formats::{ea_stream, oneshots, bnk}`. Notes: `docs/re/music.md`, `docs/re/audio.md`.
use nfs_assets::audio::{
    Countdown, LapEvent, MusicPlayer, SpeechQueue, StreamCommand, StreamStatus, on_lap_completed,
    on_race_finished, result_music, speech_volume,
};
use nfs_formats::{EaStream, decode_patch, parse_bnk, parse_ea_stream, parse_oneshots, parse_viv};

/// Random source for the original `|0x538040() + GetTickCount()|` (xorshift32, 31 bits).
#[derive(Debug, Clone)]
pub struct Rand(u32);

impl Rand {
    pub fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }
    pub fn draw(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x & 0x7fff_ffff
    }
}

/// One music stream, decoded block by block.
pub struct MusicTrackCore {
    data: Vec<u8>,
    stream: EaStream,
    next_block: usize,
}

impl MusicTrackCore {
    pub fn new(data: Vec<u8>) -> Result<Self, String> {
        let stream = parse_ea_stream(&data)?;
        Ok(Self {
            data,
            stream,
            next_block: 0,
        })
    }
    pub fn sample_rate(&self) -> u32 {
        self.stream.sample_rate
    }
    pub fn channels(&self) -> usize {
        self.stream.channels
    }
    pub fn total_frames(&self) -> u64 {
        self.stream.decoded_samples()
    }
    pub fn done(&self) -> bool {
        self.next_block >= self.stream.blocks.len()
    }
    /// Decodes whole blocks until at least `min_frames` frames (or the end); returns planar
    /// f32 (all of channel 0, then channel 1).
    pub fn decode(&mut self, min_frames: usize) -> Result<Vec<f32>, String> {
        let mut planes = vec![Vec::new(); self.stream.channels];
        while !self.done() && planes[0].len() < min_frames {
            self.stream
                .decode_block(&self.data, self.next_block, &mut planes)?;
            self.next_block += 1;
        }
        Ok(planes
            .iter()
            .flatten()
            .map(|&s| s as f32 / 32768.0)
            .collect())
    }
}

/// Command stride of `MusicDirectorCore::take_commands`: `[kind, a, b]`, kind 0 Start(file),
/// 1 SetVolume(v), 2 Fade(ms, v), 3 Stop.
pub const MUSIC_COMMAND_STRIDE: usize = 3;

pub struct MusicDirectorCore {
    pub player: MusicPlayer,
    rand: Rand,
}

impl MusicDirectorCore {
    pub fn new(files: Vec<String>, level: i32, seed: u32) -> Self {
        Self {
            player: MusicPlayer::new(files, level),
            rand: Rand::new(seed),
        }
    }
    pub fn play(&mut self, pattern: Option<&str>, mode: u8) -> usize {
        let r = &mut self.rand;
        self.player.play(pattern, mode, &mut || r.draw())
    }
    pub fn set_level(&mut self, level: i32) {
        let r = &mut self.rand;
        self.player.set_level(level, &mut || r.draw());
    }
    pub fn toggle(&mut self) {
        let r = &mut self.rand;
        self.player.toggle(&mut || r.draw());
    }
    pub fn tick(&mut self, playing: bool, volume: i32) {
        let r = &mut self.rand;
        self.player
            .tick(StreamStatus { playing, volume }, &mut || r.draw());
    }
    pub fn take_commands(&mut self) -> Vec<i32> {
        self.player
            .take_commands()
            .iter()
            .flat_map(|c| match *c {
                StreamCommand::Start { file } => [0, file as i32, 0],
                StreamCommand::SetVolume(v) => [1, v, 0],
                StreamCommand::Fade { ms, volume } => [2, ms, volume],
                StreamCommand::Stop => [3, 0, 0],
            })
            .collect()
    }
}

/// A decoded phrase: mono PCM at its sample rate (empty when missing).
pub struct PhrasePcm {
    pub name: String,
    pub sample_rate: u32,
    pub pcm: Vec<f32>,
}

/// Race announcer: phrases of `zzzxeng.viv`, countdown, lap and result calls, queue.
pub struct AnnouncerCore {
    pub phrases: Vec<PhrasePcm>,
    pub countdown: Countdown,
    pub queue: SpeechQueue,
    pub speech_level: i32,
    rand: Rand,
}

impl AnnouncerCore {
    /// Phrase table of `Speech_Load` 0x4affd0: `oneshots.txt` inside the archive.
    pub fn load(viv: &[u8], speech_level: i32, seed: u32) -> Result<Self, String> {
        let entries = parse_viv(viv)?;
        let file = |name: &str| {
            entries
                .iter()
                .find(|e| e.name.eq_ignore_ascii_case(name))
                .map(|e| &viv[e.offset..e.offset + e.size])
        };
        let table = file("oneshots.txt").ok_or("zzzxeng.viv: no oneshots.txt")?;
        let phrases = parse_oneshots(table)
            .into_iter()
            .map(|ph| {
                let decoded = file(&ph.name).and_then(|bank| {
                    let b = parse_bnk(bank).ok()?;
                    let patch = b.patch(0)?;
                    let pcm = decode_patch(bank, patch).ok()?;
                    let ch = patch.channels.max(1) as usize;
                    Some((
                        patch.sample_rate,
                        pcm.iter()
                            .step_by(ch)
                            .map(|&s| s as f32 / 32768.0)
                            .collect(),
                    ))
                });
                let (sample_rate, pcm) = decoded.unwrap_or((11025, Vec::new()));
                PhrasePcm {
                    name: ph.name,
                    sample_rate,
                    pcm,
                }
            })
            .collect();
        Ok(Self {
            phrases,
            countdown: Countdown::default(),
            queue: SpeechQueue::default(),
            speech_level,
            rand: Rand::new(seed),
        })
    }

    /// Phrase volume 0..127 (0x4a8300 / 0x4a8320).
    pub fn volume(&self) -> i32 {
        speech_volume(self.speech_level)
    }

    fn push(&mut self, phrase: u16) {
        let available = self
            .phrases
            .get(phrase as usize)
            .is_some_and(|p| !p.pcm.is_empty());
        self.queue.push(phrase, available);
    }

    /// New race: countdown and queue restart.
    pub fn reset(&mut self) {
        self.countdown = Countdown::default();
        self.queue.clear();
    }

    /// Countdown at scheduler step `step`; returns the requested phrase or −1.
    pub fn countdown(&mut self, step: i32) -> i32 {
        match self.countdown.update(step, false) {
            Some(p) => {
                self.push(p);
                p as i32
            }
            None => -1,
        }
    }

    /// `Race_OnLapSpeech` 0x4277d0 (race type 0, no time gap: approximation A54).
    pub fn lap_completed(&mut self, laps_done: u32, total: u32, cars: u32, place: u32, best: bool) {
        let e = LapEvent {
            laps_done,
            total_laps: total,
            cars,
            place,
            new_best: best,
            gap: None,
            race_type: 0,
            quiet: false,
        };
        let r = &mut self.rand;
        for p in on_lap_completed(&e, &mut || r.draw()) {
            self.push(p);
        }
    }

    /// 0x41d050 on the finish: queues the result call, returns the result music pattern
    /// (`""` for none).
    pub fn race_finished(&mut self, place: u32, cars: u32) -> &'static str {
        let r = &mut self.rand;
        for p in on_race_finished(place, cars, 0, &mut || r.draw()) {
            self.push(p);
        }
        result_music(place, cars).unwrap_or("")
    }

    /// Next phrase to start, or −1. `remaining_cs` < 0: nothing is playing.
    pub fn update(&mut self, remaining_cs: i32) -> i32 {
        self.queue
            .update((remaining_cs >= 0).then_some(remaining_cs))
            .map_or(-1, |p| p as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game_file(rel: &str) -> Option<Vec<u8>> {
        let mut dir = std::env::current_dir().ok()?;
        loop {
            if let Ok(d) = std::fs::read(dir.join("game/app").join(rel)) {
                return Some(d);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    #[test]
    fn director_flattens_commands() {
        let mut m = MusicDirectorCore::new(vec!["menu01-a-b".into(), "menu02-c-d".into()], 127, 7);
        m.player.front_end = true;
        assert_eq!(m.play(Some("menu*"), 0), 2);
        let c = m.take_commands();
        assert_eq!(&c[..3], &[1, 0, 0]);
        assert_eq!(c[3], 0);
        m.tick(true, 0);
        assert_eq!(m.take_commands(), vec![2, 5, 79]);
    }

    #[test]
    fn installed_announcer_counts_down_and_calls_laps() {
        let Some(viv) = game_file("GameData/Speech/zzzxeng.viv") else {
            eprintln!("skip: game/app not installed");
            return;
        };
        let mut a = AnnouncerCore::load(&viv, 127, 1).unwrap();
        assert_eq!(a.phrases.len(), 166);
        assert!(a.phrases[..4].iter().all(|p| p.pcm.len() > 4000));
        assert_eq!(a.volume(), 83);
        let fired: Vec<_> = (0..=0x1c0)
            .map(|s| a.countdown(s))
            .filter(|&p| p >= 0)
            .collect();
        assert_eq!(fired, [0, 1, 2, 3]);
        assert_eq!(a.update(-1), 0);
        assert_eq!(a.update(30), -1);
        assert_eq!(a.update(-1), 1);
        a.reset();
        a.lap_completed(1, 3, 4, 2, false);
        assert_eq!(a.queue.waiting(), 2);
        assert_eq!(a.race_finished(1, 4), "win*");
    }

    #[test]
    fn installed_track_decodes_in_chunks() {
        let Some(viv) = game_file("GameData/Music/zzzymus.viv") else {
            eprintln!("skip: game/app not installed");
            return;
        };
        let entries = parse_viv(&viv).unwrap();
        let e = entries
            .iter()
            .find(|e| e.name.starts_with("vic01"))
            .unwrap();
        let mut t = MusicTrackCore::new(viv[e.offset..e.offset + e.size].to_vec()).unwrap();
        assert_eq!((t.sample_rate(), t.channels()), (32000, 2));
        let mut frames = 0;
        while !t.done() {
            frames += t.decode(64000).unwrap().len() / 2;
        }
        assert_eq!(frames as u64, t.total_frames());
    }
}

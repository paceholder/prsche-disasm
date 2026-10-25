//! Original NFS5 music player (the "Music Globals" at `0x656ab4`), without an audio backend.
//!
//! Research notes: `docs/re/music.md` in the research workspace. The player is pure: the host
//! reports the stream state (`StreamStatus`), supplies the random source the original builds
//! from `0x538040() + GetTickCount()`, and executes the `StreamCommand`s it emits.

/// `0x5d14b0`: playlist volume = `MUSIC_SCALE × level >> 7` (`Audio_MusicVolume` 0x4a82f0).
pub const MUSIC_SCALE: i32 = 80;
/// Fade-out before switching to another track (`Music_Play` 0x4ae4d0, `Music_Toggle` 0x4ad790).
pub const SWITCH_FADE_MS: i32 = 500;
/// Volume ramp after a stream has started (`Music_Tick` 0x4ad9c0, state 2).
pub const START_RAMP_MS: i32 = 5;
/// Fade used when a front-end screen changes its pattern (`FEScreen_UpdateMusic` 0x4d1bd0).
pub const SCREEN_FADE_MS: i32 = 1500;
/// Second stop of a `NONE` screen (0x4d1bd0).
pub const NONE_FADE_MS: i32 = 150;
/// Playlist capacity (`u8[128]` at +0xe8).
pub const PLAYLIST_MAX: usize = 128;

/// `Audio_MusicVolume` 0x4a82f0.
pub fn music_volume(level: i32) -> i32 {
    (MUSIC_SCALE * level) >> 7
}

/// What the stream must do. Volumes are 0..127; the stream keeps its volume across `Start`
/// and `Stop`, like the original's stream object (`+0xe4`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamCommand {
    /// Open archive file `file` and play it once from the start (0x567720).
    Start { file: usize },
    /// Set the volume now (0x567810).
    SetVolume(i32),
    /// Ramp the volume linearly to `volume` over `ms` (0x566ae0).
    Fade { ms: i32, volume: i32 },
    /// Stop playback (0x567740).
    Stop,
}

/// Stream state as the tick reads it: `playing` = `+0x8c` (0x566a40), `volume` = 0x567850.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StreamStatus {
    pub playing: bool,
    pub volume: i32,
}

/// Switch state `+0x14`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchState {
    /// 0: idle or playing.
    Idle,
    /// 1: fade out, then start the pending track.
    Pending,
    /// 2: started, volume ramp pending.
    Started,
    /// −1: fade out, then idle.
    Stopping,
}

#[derive(Debug, Clone)]
pub struct MusicPlayer {
    /// Archive file names, by TOC index.
    files: Vec<String>,
    /// `+0x00`: playlist volume 0..127 (0 = music off).
    volume: i32,
    /// `g_MusicLevel` 0x5d1740: used as is by `lose*`/`win*`/`vic*` tracks.
    level: i32,
    /// `+0xe8`, `+0x08`.
    playlist: [u8; PLAYLIST_MAX],
    len: usize,
    /// `+0x0c`.
    index: i32,
    /// `+0x10`.
    on: bool,
    /// `+0x14`.
    state: SwitchState,
    /// `+0x18`.
    changed: bool,
    /// `+0x20`.
    fade_ms: i32,
    /// `+0x24`.
    shuffle: bool,
    /// `+0x28`: file of the open stream.
    current: Option<usize>,
    /// `0x65b928 != 0`: the front end runs (shuffle always on).
    pub front_end: bool,
    /// `g_RandomPlaylist` 0x657a5c (`fe.txt randomplaylist`, default 0).
    pub random_playlist: bool,
    status: StreamStatus,
    commands: Vec<StreamCommand>,
}

impl MusicPlayer {
    /// State after `Music_Init` 0x4ae100 / `Music_ResetState` 0x4adf50 with music level `level`.
    pub fn new(files: Vec<String>, level: i32) -> Self {
        let level = level.clamp(0, 127);
        Self {
            files,
            volume: music_volume(level),
            level,
            playlist: [0; PLAYLIST_MAX],
            len: 0,
            index: -1,
            on: false,
            state: SwitchState::Idle,
            changed: false,
            fade_ms: 0,
            shuffle: false,
            current: None,
            front_end: false,
            random_playlist: false,
            status: StreamStatus::default(),
            commands: Vec::new(),
        }
    }

    pub fn files(&self) -> &[String] {
        &self.files
    }
    pub fn volume(&self) -> i32 {
        self.volume
    }
    pub fn level(&self) -> i32 {
        self.level
    }
    pub fn playlist(&self) -> &[u8] {
        &self.playlist[..self.len]
    }
    pub fn index(&self) -> i32 {
        self.index
    }
    pub fn state(&self) -> SwitchState {
        self.state
    }
    pub fn is_on(&self) -> bool {
        self.on
    }
    /// File of the open stream, if any.
    pub fn current_file(&self) -> Option<usize> {
        self.current
    }
    /// `+0x18`, cleared on read (`Music_NowPlaying` 0x4ad6f0).
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }
    pub fn take_commands(&mut self) -> Vec<StreamCommand> {
        std::mem::take(&mut self.commands)
    }
    /// Latest stream state, used by `set_volume` between ticks.
    pub fn set_status(&mut self, status: StreamStatus) {
        self.status = status;
    }

    /// `Music_SetPlaylist` 0x4ae470: archive indices; out-of-range ids are skipped.
    pub fn set_playlist(&mut self, ids: &[i32]) {
        self.len = 0;
        for &id in ids {
            if id >= 0 && (id as usize) < self.files.len() && self.len < PLAYLIST_MAX {
                self.playlist[self.len] = id as u8;
                self.len += 1;
            }
        }
    }

    fn fill_playlist(&mut self, pattern: &str) {
        self.len = 0;
        for i in 0..self.files.len() {
            if self.len >= PLAYLIST_MAX {
                break;
            }
            if glob_match(&self.files[i], pattern) {
                self.playlist[self.len] = i as u8;
                self.len += 1;
            }
        }
    }

    fn open_current(&mut self) {
        let file = self.playlist[self.index as usize] as usize;
        self.current = Some(file);
        self.commands.push(StreamCommand::Start { file });
        self.state = SwitchState::Started;
    }

    /// `Music_Play` 0x4ae4d0. `mode` 0: play the playlist; 1: one random match after the
    /// current playlist (result jingle); 2: one random match and nothing after it. Returns the
    /// playlist length.
    pub fn play(&mut self, pattern: Option<&str>, mode: u8, rng: &mut dyn FnMut() -> u32) -> usize {
        let saved_len = self.len;
        let saved = self.playlist;
        if let Some(p) = pattern {
            self.fill_playlist(p);
            if self.len == 0 && p.len() < 0x3d {
                self.fill_playlist(&format!("*-{p}"));
            }
        }
        if self.volume == 0 || self.len == 0 {
            return self.len.min(self.volume as usize);
        }
        self.shuffle = self.front_end || self.random_playlist;
        let n = self.len as i64;
        let r = |rng: &mut dyn FnMut() -> u32| (rng() & 0x7fff_ffff) as i64;
        let next = if mode == 1 {
            let pick = self.playlist[(r(rng) % n) as usize];
            self.playlist[..saved_len].copy_from_slice(&saved[..saved_len]);
            if saved_len < PLAYLIST_MAX {
                self.playlist[saved_len] = pick;
            }
            self.len = saved_len;
            saved_len as i32
        } else if mode == 2 {
            self.playlist[0] = self.playlist[(r(rng) % n) as usize];
            self.len = 0;
            0
        } else if n == 1 {
            0
        } else if pattern.is_none() {
            ((r(rng) % (n - 1) + 1 + self.index as i64) % n) as i32
        } else {
            (r(rng) % n) as i32
        };
        match self.state {
            SwitchState::Idle if self.index < 0 => {
                self.fade_ms = 0;
                self.commands.push(StreamCommand::SetVolume(0));
                self.index = next;
                self.open_current();
            }
            SwitchState::Idle => {
                self.commands.push(StreamCommand::Fade {
                    ms: SWITCH_FADE_MS,
                    volume: 0,
                });
                self.fade_ms = SWITCH_FADE_MS;
                self.state = SwitchState::Pending;
                self.index = next;
                self.current = None;
            }
            _ => {
                self.state = SwitchState::Pending;
                self.index = next;
            }
        }
        self.on = true;
        if pattern.is_some() {
            self.changed = true;
        }
        self.len
    }

    /// `Music_Stop` 0x4ae3c0: `ms == 0` stops at once, else fades out over `ms`.
    pub fn stop(&mut self, ms: i32) {
        if self.index < 0 {
            return;
        }
        if ms == 0 {
            self.commands.push(StreamCommand::Stop);
            self.fade_ms = 0;
            self.current = None;
            self.state = SwitchState::Idle;
            self.index = -1;
            return;
        }
        if self.state == SwitchState::Idle {
            self.commands.push(StreamCommand::Fade { ms, volume: 0 });
            self.fade_ms = ms;
            self.current = None;
        }
        self.state = SwitchState::Stopping;
        self.index = -1;
    }

    /// `Music_SetVolume` 0x4ae8c0 (0..127 stream volume, see `music_volume`).
    pub fn set_volume(&mut self, v: i32, rng: &mut dyn FnMut() -> u32) {
        let v = v.clamp(0, 127);
        if self.volume == v {
            return;
        }
        if v == 0 {
            self.volume = 0;
            self.stop(0);
            return;
        }
        let cur = self.status.volume;
        let mut rescaled = false;
        if self.fade_ms != 0 && cur > 0 && self.volume > 0 {
            let t = self.fade_ms * cur / self.volume;
            if t != 0 {
                self.commands
                    .push(StreamCommand::SetVolume(t * v / self.fade_ms));
                self.commands.push(StreamCommand::Fade { ms: t, volume: 0 });
                rescaled = true;
            }
        }
        if !rescaled && self.state != SwitchState::Started {
            self.commands.push(StreamCommand::SetVolume(v));
        }
        let old = self.volume;
        self.volume = v;
        if old == 0 {
            self.play(None, 0, rng);
        }
    }

    /// Music level 0..127 (option `Audio.MusicLevel`): `g_MusicLevel` and the playlist volume.
    pub fn set_level(&mut self, level: i32, rng: &mut dyn FnMut() -> u32) {
        self.level = level.clamp(0, 127);
        self.set_volume(music_volume(self.level), rng);
    }

    /// `Music_Toggle` 0x4ad790 (race key action 0x12).
    pub fn toggle(&mut self, rng: &mut dyn FnMut() -> u32) {
        if self.len == 0 || self.volume <= 0 {
            return;
        }
        self.changed = true;
        if self.state == SwitchState::Idle && self.index >= 0 {
            self.commands.push(StreamCommand::Fade {
                ms: SWITCH_FADE_MS,
                volume: 0,
            });
            self.fade_ms = 1000;
        }
        self.index += 1;
        if self.on {
            self.on = false;
            self.state = SwitchState::Stopping;
            self.index = -1;
            return;
        }
        self.on = true;
        self.state = SwitchState::Pending;
        self.index = if self.len == 1 {
            0
        } else {
            ((rng() & 0x7fff_ffff) as usize % self.len) as i32
        };
    }

    /// `lose*`, `win*`, `vic*` tracks play at the music level itself (0x4adbf9..0x4adc70).
    fn is_jingle(name: &str) -> bool {
        let n = name.as_bytes();
        n.starts_with(b"lose") || n.starts_with(b"win") || n.starts_with(b"vic")
    }

    /// `Music_Tick` 0x4ad9c0 (sync task, every 25 audio-library ticks).
    pub fn tick(&mut self, status: StreamStatus, rng: &mut dyn FnMut() -> u32) {
        self.status = status;
        match self.state {
            SwitchState::Idle => {
                if status.playing || self.index < 0 {
                    return;
                }
                let n = self.len as i64;
                if n == 1 {
                    self.index = 0;
                } else if n > 0 {
                    let i = self.index as i64;
                    self.index = if self.shuffle {
                        (((rng() & 0x7fff_ffff) as i64 % (n - 1) + 1 + i) % n) as i32
                    } else {
                        ((i + 1) % n) as i32
                    };
                }
                self.commands.push(StreamCommand::SetVolume(0));
                if n > 0 {
                    self.open_current();
                    self.changed = true;
                    self.on = true;
                } else {
                    // Victory (mode 2): nothing follows. The original keeps the index and
                    // repeats the volume-0 call every tick; ending here is equivalent.
                    self.index = -1;
                    self.current = None;
                }
            }
            SwitchState::Started => {
                if !status.playing {
                    return;
                }
                let jingle = self
                    .current
                    .and_then(|f| self.files.get(f))
                    .is_some_and(|n| Self::is_jingle(n));
                let volume = if jingle { self.level } else { self.volume };
                self.commands.push(StreamCommand::Fade {
                    ms: START_RAMP_MS,
                    volume,
                });
                self.state = SwitchState::Idle;
            }
            SwitchState::Pending | SwitchState::Stopping => {
                if status.playing && status.volume != 0 {
                    return;
                }
                self.commands.push(StreamCommand::Stop);
                self.current = None;
                if self.state == SwitchState::Pending {
                    self.fade_ms = 0;
                    self.open_current();
                } else {
                    self.state = SwitchState::Idle;
                }
            }
        }
    }
}

/// `Glob_Match` 0x4a5f90: case-insensitive `*`, `?`, `#` (digit); the rightmost of the
/// operators `& | ! ~ ^` splits the pattern (`!` and `~` act as `|` there, as in the original).
pub fn glob_match(name: &str, pattern: &str) -> bool {
    if let Some(at) = pattern.rfind(['&', '|', '!', '~', '^']) {
        let (l, r) = (&pattern[..at], &pattern[at + 1..]);
        return match pattern.as_bytes()[at] {
            b'&' => glob_match(name, l) && glob_match(name, r),
            b'^' => glob_match(name, r) ^ glob_match(name, l),
            _ => glob_match(name, l) || glob_match(name, r),
        };
    }
    glob_bytes(name.as_bytes(), pattern.as_bytes())
}

fn glob_bytes(name: &[u8], pat: &[u8]) -> bool {
    let (mut n, mut p) = (0, 0);
    loop {
        let pc = pat.get(p).copied().unwrap_or(0);
        let nc = name.get(n).copied().unwrap_or(0);
        match pc {
            b'*' => {
                let rest = &pat[p + 1..];
                return (n..=name.len()).any(|k| glob_bytes(&name[k..], rest));
            }
            b'?' => {
                if nc == 0 {
                    return false;
                }
            }
            b'#' => {
                if !nc.is_ascii_digit() {
                    return false;
                }
            }
            _ => {
                if !nc.eq_ignore_ascii_case(&pc) {
                    return false;
                }
                if pc == 0 {
                    return true;
                }
            }
        }
        p += 1;
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// zzzymus.viv TOC order (data zzzymus.viv).
    fn files() -> Vec<String> {
        [
            "game13-Activator-Morphadron",
            "game03-Choose Your Enemy-Cypher",
            "game10-Funky Phreakout-Morphadron",
            "game11-Injector-Cypher",
            "game04-Let the Music Move You-Morphadron",
            "game02-MetroGnome-Captain Ginger",
            "game01-Rezidue-Morphadron",
            "game08-Rock This Place-Morphadron",
            "game07-R U Ready-Morphadron",
            "game06-Sentient-Cypher",
            "game14-Stealth Run-Morphadron",
            "game12-The Moebius-Morphadron",
            "game05-The UK Sound-Morphadron",
            "game09-Twin-Cypher",
            "vic01-Congrats-CaptainGinger",
            "lose01-Aquadelic-Captain Ginger",
            "lose02-Hack the Planet-Morphadron",
            "lose03-Loser-Morphadron",
            "lose04-Sub Zero-Cypher",
            "menu01-Psychonaught-Captain Ginger",
            "menu02-Aircon-Cypher",
            "menu03-Dr. Know-Morphadron",
            "menu04-Cold Fusion Power-Morphadron",
            "menu05-Orion is Lonely-Cypher",
            "win01-Warm Storage-Morphadron",
            "win02-Champion-Cypher",
            "win03-Gimme the Power-Morphadron",
            "win04-Victory-Cypher",
        ]
        .map(String::from)
        .to_vec()
    }

    fn seq(values: &[u32]) -> impl FnMut() -> u32 + '_ {
        let mut i = 0;
        move || {
            let v = values[i % values.len()];
            i += 1;
            v
        }
    }

    const PLAYING: StreamStatus = StreamStatus {
        playing: true,
        volume: 79,
    };
    const ENDED: StreamStatus = StreamStatus {
        playing: false,
        volume: 79,
    };
    const SILENT: StreamStatus = StreamStatus {
        playing: true,
        volume: 0,
    };

    fn race_player() -> MusicPlayer {
        let mut m = MusicPlayer::new(files(), 127);
        m.set_playlist(&(0..11).collect::<Vec<_>>());
        m
    }

    #[test]
    fn glob_rules() {
        assert!(glob_match("menu03-Dr. Know-Morphadron", "menu*"));
        assert!(glob_match("MENU03-x", "menu*"));
        assert!(!glob_match("game01-Rezidue", "menu*"));
        assert!(glob_match("game01-Rezidue", "game##-*"));
        assert!(glob_match("win01-a", "win*|lose*"));
        assert!(glob_match("lose01-a", "win*|lose*"));
        assert!(!glob_match("win01-a", "win*&lose*"));
        assert!(glob_match("ab", "a?"));
        assert!(!glob_match("a", "a?"));
        assert!(!glob_match("win01-a", "win*^w*"));
    }

    #[test]
    fn volume_law() {
        assert_eq!(music_volume(127), 79);
        assert_eq!(music_volume(64), 40);
        assert_eq!(MusicPlayer::new(files(), 127).volume(), 79);
    }

    #[test]
    fn menu_pattern_starts_at_once_then_ramps_in_5_ms() {
        let mut m = MusicPlayer::new(files(), 127);
        m.front_end = true;
        assert_eq!(m.play(Some("menu*"), 0, &mut seq(&[7])), 5);
        assert_eq!(m.playlist(), &[19, 20, 21, 22, 23]);
        // rand % 5 = 2 -> menu03.
        assert_eq!(
            m.take_commands(),
            vec![
                StreamCommand::SetVolume(0),
                StreamCommand::Start { file: 21 }
            ]
        );
        m.tick(ENDED, &mut seq(&[0])); // not playing yet: waits
        assert!(m.take_commands().is_empty());
        m.tick(PLAYING, &mut seq(&[0]));
        assert_eq!(
            m.take_commands(),
            vec![StreamCommand::Fade { ms: 5, volume: 79 }]
        );
        assert_eq!(m.state(), SwitchState::Idle);
    }

    #[test]
    fn no_match_retries_with_title() {
        let mut m = MusicPlayer::new(files(), 127);
        assert_eq!(m.play(Some("Rezidue*"), 0, &mut seq(&[0])), 1);
        assert_eq!(m.playlist(), &[6]);
        assert_eq!(m.play(Some("dogface"), 0, &mut seq(&[0])), 0);
    }

    #[test]
    fn race_start_never_picks_the_last_entry() {
        for r in 0..40u32 {
            let mut m = race_player();
            m.play(None, 0, &mut seq(&[r]));
            assert!((0..10).contains(&m.index()), "rand {r}: {}", m.index());
            assert_eq!(m.index() as u32, r % 10);
        }
    }

    #[test]
    fn sequential_playlist_loops() {
        let mut m = race_player();
        m.play(None, 0, &mut seq(&[9])); // index 9
        m.tick(PLAYING, &mut seq(&[0]));
        m.take_commands();
        m.tick(ENDED, &mut seq(&[0]));
        assert_eq!(m.index(), 10);
        assert_eq!(
            m.take_commands(),
            vec![
                StreamCommand::SetVolume(0),
                StreamCommand::Start { file: 10 }
            ]
        );
        m.tick(PLAYING, &mut seq(&[0]));
        m.tick(ENDED, &mut seq(&[0]));
        assert_eq!(m.index(), 0, "after the last entry the first one");
    }

    #[test]
    fn shuffle_never_repeats() {
        let mut m = MusicPlayer::new(files(), 127);
        m.front_end = true;
        m.play(Some("menu*"), 0, &mut seq(&[0]));
        for r in 0..20u32 {
            let before = m.index();
            m.tick(PLAYING, &mut seq(&[r]));
            m.tick(ENDED, &mut seq(&[r]));
            assert_ne!(m.index(), before);
        }
    }

    #[test]
    fn switching_fades_out_500_ms_first() {
        let mut m = race_player();
        m.play(None, 0, &mut seq(&[3]));
        m.tick(PLAYING, &mut seq(&[0]));
        m.take_commands();
        m.play(Some("win*"), 0, &mut seq(&[1]));
        assert_eq!(
            m.take_commands(),
            vec![StreamCommand::Fade { ms: 500, volume: 0 }]
        );
        assert_eq!(m.state(), SwitchState::Pending);
        m.tick(PLAYING, &mut seq(&[0])); // still audible
        assert!(m.take_commands().is_empty());
        m.tick(SILENT, &mut seq(&[0]));
        assert_eq!(
            m.take_commands(),
            vec![StreamCommand::Stop, StreamCommand::Start { file: 25 }]
        );
    }

    #[test]
    fn result_jingle_then_playlist() {
        let mut m = race_player();
        m.play(None, 0, &mut seq(&[4])); // index 4
        m.tick(PLAYING, &mut seq(&[0]));
        m.take_commands();
        assert_eq!(m.play(Some("win*"), 1, &mut seq(&[2])), 11);
        assert_eq!(m.index(), 11);
        m.tick(SILENT, &mut seq(&[0]));
        assert_eq!(
            m.take_commands(),
            vec![
                StreamCommand::Fade { ms: 500, volume: 0 },
                StreamCommand::Stop,
                StreamCommand::Start { file: 26 }
            ]
        );
        // Jingles use the music level, not 80 × level >> 7.
        m.tick(PLAYING, &mut seq(&[0]));
        assert_eq!(
            m.take_commands(),
            vec![StreamCommand::Fade { ms: 5, volume: 127 }]
        );
        m.tick(ENDED, &mut seq(&[0]));
        assert_eq!(m.index(), 1, "(n + 1) % n");
        assert_eq!(m.current_file(), Some(1));
    }

    #[test]
    fn victory_plays_once() {
        let mut m = MusicPlayer::new(files(), 127);
        m.play(Some("vic*"), 2, &mut seq(&[0]));
        assert_eq!(m.current_file(), Some(14));
        m.tick(PLAYING, &mut seq(&[0]));
        m.tick(ENDED, &mut seq(&[0]));
        assert_eq!(m.current_file(), None);
        m.tick(ENDED, &mut seq(&[0]));
        assert!(!m
            .take_commands()
            .iter()
            .any(|c| matches!(c, StreamCommand::Start { file } if *file != 14)));
    }

    #[test]
    fn stop_now_and_with_fade() {
        let mut m = race_player();
        m.play(None, 0, &mut seq(&[0]));
        m.tick(PLAYING, &mut seq(&[0]));
        m.take_commands();
        m.stop(1500);
        assert_eq!(
            m.take_commands(),
            vec![StreamCommand::Fade {
                ms: 1500,
                volume: 0
            }]
        );
        m.tick(SILENT, &mut seq(&[0]));
        assert_eq!(m.take_commands(), vec![StreamCommand::Stop]);
        assert_eq!((m.state(), m.index()), (SwitchState::Idle, -1));
        // Resume after a pause picks a new track: rand % (n - 1).
        m.play(None, 0, &mut seq(&[5]));
        assert_eq!(m.index(), 5);
        m.stop(0);
        assert_eq!(m.take_commands().last(), Some(&StreamCommand::Stop));
    }

    #[test]
    fn level_zero_stops_and_raising_restarts() {
        let mut m = race_player();
        m.play(None, 0, &mut seq(&[0]));
        m.tick(PLAYING, &mut seq(&[0]));
        m.take_commands();
        m.set_level(64, &mut seq(&[0]));
        assert_eq!(m.take_commands(), vec![StreamCommand::SetVolume(40)]);
        m.set_level(0, &mut seq(&[0]));
        assert_eq!(m.take_commands(), vec![StreamCommand::Stop]);
        assert_eq!(
            m.play(None, 0, &mut seq(&[0])),
            0,
            "volume 0: nothing starts"
        );
        m.set_level(127, &mut seq(&[3]));
        assert_eq!(m.index(), 3);
        assert!(m
            .take_commands()
            .contains(&StreamCommand::Start { file: 3 }));
    }

    #[test]
    fn toggle_turns_music_off_and_on() {
        let mut m = race_player();
        m.play(None, 0, &mut seq(&[0]));
        m.tick(PLAYING, &mut seq(&[0]));
        m.take_commands();
        m.toggle(&mut seq(&[0]));
        assert_eq!(m.state(), SwitchState::Stopping);
        m.tick(SILENT, &mut seq(&[0]));
        m.toggle(&mut seq(&[7]));
        assert_eq!((m.state(), m.index()), (SwitchState::Pending, 7));
    }
}

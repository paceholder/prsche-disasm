//! `EngineMixer` against the original `EngineAudio_PushState 0x4ac780` / `EngineAudio_Mix
//! 0x4ac290`, executed in Unicorn by `scripts/research/replay-engine-audio.py` (fixtures in
//! `local/fixtures/engine-audio-{steps,calls}.tsv`). The replay rebuilds the recorder's
//! state (sd1a / sd1a16 tables, `356Acabrio16.spl`) and its fake audio library (handles from
//! 100, one-shot timers per slot); it is skipped without `local/game`.
#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
#[allow(unused_imports)]
use local_fixtures::Fixture;

use nfs_assets::audio::{
    engine_tables_from_viv, EngineMixer, MixerGlobals, OneShotClock, PushStateArgs,
    ThrottleSmoother, VoiceCommand, VoiceHost, VoiceId, ENGINE_VOICES, XFADE_TABLE,
    XFADE_TABLE_FILE_OFFSET,
};
use nfs_formats::{parse_spl, CrdLayer, CrdTable, EngineSpl};
use std::collections::HashMap;
use std::path::PathBuf;

/// Bank handle the recorder puts at state+0xD0; the first argument of every `play`.
const BANK_HANDLE: &str = "3";
/// First handle returned by the recorder's fake library.
const FIRST_HANDLE: i64 = 100;

fn game_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game");
    dir.join("GameData/Sounds").is_dir().then_some(dir)
}

struct Tsv {
    header: HashMap<String, usize>,
    rows: Vec<Vec<String>>,
}

impl Tsv {
    fn read(name: &str) -> Self {
        let text = std::fs::read_to_string(local_fixtures::dir().join(name)).unwrap();
        let mut lines = text.lines();
        let header = lines
            .next()
            .unwrap()
            .split('\t')
            .enumerate()
            .map(|(i, k)| (k.to_string(), i))
            .collect();
        let rows = lines
            .map(|l| l.split('\t').map(str::to_string).collect())
            .collect();
        Self { header, rows }
    }
    fn get<'a>(&self, row: &'a [String], key: &str) -> &'a str {
        &row[self.header[key]]
    }
    fn int(&self, row: &[String], key: &str) -> i64 {
        self.get(row, key).parse().unwrap()
    }
}

/// The recorder's `FakeAudio`: handles from 100, one-shot length per slot, `step_cs` per update.
struct FakeAudio {
    oneshot_cs: HashMap<u8, i32>,
    /// Remaining 1/100 s; `None` = looping.
    voices: HashMap<VoiceId, Option<i32>>,
    log: Vec<[String; 9]>,
    params: Vec<(u8, u8, u16, i16)>,
}

fn handle(v: VoiceId) -> String {
    (FIRST_HANDLE + v.0 as i64).to_string()
}

impl FakeAudio {
    fn new(oneshot_cs: &[(u8, i32)]) -> Self {
        Self {
            oneshot_cs: oneshot_cs.iter().copied().collect(),
            voices: HashMap::new(),
            log: Vec::new(),
            params: Vec::new(),
        }
    }
    fn advance(&mut self, cs: i32) {
        for rem in self.voices.values_mut().flatten() {
            *rem -= cs;
        }
    }
    fn record(
        &mut self,
        func: &str,
        args: [String; 3],
        ret: String,
        p: Option<(u8, u8, u16, i16)>,
    ) {
        let [a0, a1, a2] = args;
        let (pv, pf, pp, pn) = match p {
            Some((v, f, pi, pa)) => (v.to_string(), f.to_string(), pi.to_string(), pa.to_string()),
            None => Default::default(),
        };
        self.log
            .push([func.into(), a0, a1, a2, ret, pv, pf, pp, pn]);
    }
}

impl OneShotClock for FakeAudio {
    fn is_finished(&mut self, voice: VoiceId) -> bool {
        let finished = match self.voices.get(&voice) {
            None => true,
            Some(rem) => rem.is_some_and(|r| r <= 0),
        };
        self.record(
            "is_finished",
            [handle(voice), String::new(), String::new()],
            (finished as i32).to_string(),
            None,
        );
        finished
    }
    fn time_remaining_cs(&mut self, voice: VoiceId) -> i32 {
        let rem = match self.voices.get(&voice) {
            None => -8,
            Some(None) => 0x7fff_ffff,
            Some(Some(r)) => (*r).max(0),
        };
        self.record(
            "time_remaining",
            [handle(voice), String::new(), String::new()],
            rem.to_string(),
            None,
        );
        rem
    }
}

impl VoiceHost for FakeAudio {
    fn command(&mut self, c: VoiceCommand) {
        let e = String::new;
        match c {
            VoiceCommand::Start {
                voice,
                slot,
                volume,
                pitch,
                pan,
                looped,
            } => {
                self.voices
                    .insert(voice, self.oneshot_cs.get(&slot).copied());
                let p = (volume, looped as u8, pitch, pan);
                self.params.push(p);
                self.record(
                    "play",
                    [BANK_HANDLE.into(), slot.to_string(), e()],
                    handle(voice),
                    Some(p),
                );
            }
            VoiceCommand::Stop { voice } => {
                self.voices.remove(&voice);
                self.record("stop", [handle(voice), e(), e()], "0".into(), None);
            }
            VoiceCommand::SetVolume { voice, volume } => self.record(
                "set_volume",
                [handle(voice), volume.to_string(), e()],
                "0".into(),
                None,
            ),
            VoiceCommand::SetPan { voice, pan } => self.record(
                "set_pan",
                [handle(voice), pan.to_string(), "0".into()],
                "0".into(),
                None,
            ),
            VoiceCommand::SetPitch { voice, pitch } => self.record(
                "set_pitch",
                [handle(voice), pitch.to_string(), e()],
                "0".into(),
                None,
            ),
        }
    }
}

/// Per-case setup of `cases()` in the recorder that is not in the TSV:
/// (name, running at start, one-shot lengths by slot, 1/100 s per update).
type CaseSetup = (&'static str, bool, &'static [(u8, i32)], i32);
const CASES: &[CaseSetup] = &[
    ("start_chase", false, &[(8, 600)], 8),
    ("start_cockpit", false, &[(6, 300)], 8),
    ("stop_chase", true, &[(11, 300)], 8),
    ("stop_restart_bumper", true, &[(7, 200), (10, 300)], 8),
];

fn opt_handle(v: Option<VoiceId>) -> i64 {
    v.map_or(-1, |v| FIRST_HANDLE + v.0 as i64)
}

#[test]
fn replays_original_mixer_fixtures_exactly() {
    skip_without_local_files!("engine-audio-calls.tsv", "engine-audio-steps.tsv");
    let Some(game) = game_dir() else {
        eprintln!("skip: local/game not installed");
        return;
    };
    let sounds = game.join("GameData/Sounds");
    let spl = parse_spl(&std::fs::read(sounds.join("356Acabrio16.spl")).unwrap()).unwrap();
    let tables = [
        engine_tables_from_viv(&std::fs::read(sounds.join("sd1a.viv")).unwrap()).unwrap(),
        engine_tables_from_viv(&std::fs::read(sounds.join("sd1a16.viv")).unwrap()).unwrap(),
    ];
    let steps = Tsv::read("engine-audio-steps.tsv");
    let calls = Tsv::read("engine-audio-calls.tsv");
    let mut expected_calls: HashMap<(String, i64), Vec<&Vec<String>>> = HashMap::new();
    for row in &calls.rows {
        let key = (calls.get(row, "case").to_string(), calls.int(row, "step"));
        expected_calls.entry(key).or_default().push(row);
    }
    // Audio_InitPlayParams defaults: every play's params equal this outside the set fields.
    let template = {
        let row = calls
            .rows
            .iter()
            .find(|r| calls.get(r, "func") == "play")
            .unwrap();
        hex(calls.get(row, "params_hex"))
    };

    let (mut mixer, mut audio, mut case) =
        (None::<EngineMixer>, FakeAudio::new(&[]), String::new());
    let mut step_cs = 0;
    let (mut rows_checked, mut calls_checked) = (0, 0);
    for row in &steps.rows {
        let name = steps.get(row, "case");
        if name != case {
            case = name.to_string();
            let (running, oneshot, cs) = CASES
                .iter()
                .find(|c| c.0 == name)
                .map_or((true, &[][..], 0), |c| (c.1, c.2, c.3));
            let detail = steps.int(row, "detail") as i32;
            let globals = MixerGlobals {
                detail,
                split_screen: false,
                spread_source: steps.int(row, "g_spread") as i32,
            };
            let mut m = EngineMixer::new(spl, &tables[(detail >= 2) as usize], globals);
            m.set_running(running);
            mixer = Some(m);
            audio = FakeAudio::new(oneshot);
            step_cs = cs;
        }
        let m = mixer.as_mut().unwrap();
        let step = steps.int(row, "step");
        let at = format!("{case} step {step}");
        let i = |k: &str| steps.int(row, k) as i32;
        let args = PushStateArgs {
            volume: i("volume"),
            rpm16: i("rpm16"),
            load: i("load"),
            mode: i("mode"),
            doppler: i("doppler"),
            pan: i("pan"),
            front_back: i("fb"),
        };
        audio.advance(step_cs);
        audio.log.clear();
        audio.params.clear();
        m.update_with(&args, &mut audio);

        let f = m.frame();
        let (out_pan, out_spread, out_pitch) = m.output_pan_spread_pitch();
        let actual: Vec<(&str, i64)> = vec![
            ("gain", m.gain() as i64),
            ("master_pre", m.master_before_gain() as i64),
            ("ring_master", f.master as i64),
            ("ring_pitch", f.pitch as i64),
            ("ring_pan", f.pan as i64),
            ("ring_spread", f.spread as i64),
            ("ring_bin", f.bin as i64),
            ("ring_group", f.group as i64),
            ("ring_load", f.load as i64),
            ("ring_mode", f.mode as i64),
            ("out_pan", out_pan as i64),
            ("out_spread", out_spread as i64),
            ("out_pitch", out_pitch as i64),
            ("start_voice", opt_handle(m.start_voice())),
            ("stop_voice", opt_handle(m.stop_voice())),
            ("running", m.running() as i64),
            ("calls", audio.log.len() as i64),
        ];
        for (k, v) in actual {
            assert_eq!(v, steps.int(row, k), "{at}: {k}");
        }
        for (v, vol) in m.volumes().iter().enumerate() {
            assert_eq!(
                *vol as i64,
                steps.int(row, &format!("vol{v}")),
                "{at}: vol{v}"
            );
        }

        let expected = expected_calls
            .remove(&(case.clone(), step))
            .unwrap_or_default();
        assert_eq!(audio.log.len(), expected.len(), "{at}: call count");
        let cols = [
            "func", "a0", "a1", "a2", "ret", "p_volume", "p_flag", "p_pitch", "p_pan",
        ];
        let mut plays = audio.params.iter();
        for (n, (got, want)) in audio.log.iter().zip(&expected).enumerate() {
            let want_fields = cols.map(|c| calls.get(want, c).to_string());
            assert_eq!(got, &want_fields, "{at}: call {n}");
            if got[0] == "play" {
                let (volume, flag, pitch, pan) = *plays.next().unwrap();
                let mut p = template.clone();
                p[8] = volume;
                p[0xb] = flag;
                p[0xc..0xe].copy_from_slice(&pitch.to_le_bytes());
                p[0x18..0x1a].copy_from_slice(&pan.to_le_bytes());
                let mut want_p = hex(calls.get(want, "params_hex"));
                // +0x16 is not written by Audio_InitPlayParams 0x561960: stale stack bytes.
                p[0x16..0x18].fill(0);
                want_p[0x16..0x18].fill(0);
                assert_eq!(p, want_p, "{at}: call {n} params");
            }
        }
        rows_checked += 1;
        calls_checked += expected.len();
    }
    assert!(expected_calls.is_empty(), "calls for unknown steps");
    assert_eq!((rows_checked, calls_checked), (1681, 8787));
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn xfade_table_equals_nfs5_exe_bytes() {
    let Some(game) = game_dir() else {
        eprintln!("skip: local/game not installed");
        return;
    };
    let exe = std::fs::read(game.join("nfs5.exe")).unwrap();
    assert_eq!(
        &exe[XFADE_TABLE_FILE_OFFSET..XFADE_TABLE_FILE_OFFSET + XFADE_TABLE.len()],
        &XFADE_TABLE[..]
    );
}

// ---- Spec scenarios (docs: openspec change original-engine-sound, specs/engine-sound) ----

/// Clock for running engines: no one-shot is ever queried after the start sample.
struct Running;
impl OneShotClock for Running {
    fn is_finished(&mut self, _: VoiceId) -> bool {
        true
    }
    fn time_remaining_cs(&mut self, _: VoiceId) -> i32 {
        0
    }
}

/// sd1a with `356Acabrio16.spl` when installed, otherwise synthetic tables: four layers per
/// table with overlapping RPM ranges, slots 0x00.. below and 0x40.. above the group split.
fn scenario_mixer() -> EngineMixer {
    let installed = game_dir().map(|g| {
        let s = g.join("GameData/Sounds");
        (
            parse_spl(&std::fs::read(s.join("356Acabrio16.spl")).unwrap()).unwrap(),
            engine_tables_from_viv(&std::fs::read(s.join("sd1a.viv")).unwrap()).unwrap(),
        )
    });
    let (spl, tables) = installed.unwrap_or_else(|| {
        let table = |base: u8| CrdTable {
            version: 2,
            header: vec![0; 0x18],
            layers: (0..8)
                .map(|j| {
                    (j < 4).then(|| {
                        let mut volume = vec![0u8; 512];
                        volume[j * 100..(j * 100 + 200).min(512)].fill(100);
                        CrdLayer {
                            slot: base + j as u8 + if j % 2 == 1 { 0x40 } else { 0 },
                            volume,
                            aux: vec![0; 512],
                        }
                    })
                })
                .collect(),
        };
        (
            EngineSpl::DEFAULT,
            [table(0), table(4), table(8), table(12)],
        )
    });
    let mut m = EngineMixer::new(spl, &tables, MixerGlobals::default());
    m.set_running(true);
    m
}

fn args(rpm16: i32, load: i32) -> PushStateArgs {
    PushStateArgs {
        volume: 104,
        rpm16,
        load,
        mode: 3,
        ..PushStateArgs::default()
    }
}

#[test]
fn pitch_at_redline() {
    let mut m = EngineMixer::new(
        EngineSpl::DEFAULT, // spl[8] = 80
        &scenario_tables_empty(),
        MixerGlobals::default(),
    );
    m.set_running(true);
    m.update(&args(0x10000, 64), &mut Running);
    // 80 × 0x13333 >> 10 = 6143: ×1.4998 (1.5 within one 4.12 step).
    assert_eq!(m.frame().pitch, 6143);
    assert!((m.frame().pitch as f64 / 4096.0 - 1.5).abs() <= 1.0 / 4096.0);

    let mut m = scenario_mixer();
    let cmds = m.update(&args(0x10000, 64), &mut Running);
    let pitch = m.frame().pitch;
    let starts: Vec<_> = cmds
        .iter()
        .filter_map(|c| match c {
            VoiceCommand::Start {
                pitch,
                looped: true,
                ..
            } => Some(*pitch),
            _ => None,
        })
        .collect();
    assert!(!starts.is_empty());
    assert!(
        starts.iter().all(|&p| p == pitch),
        "all layers share one pitch"
    );
}

fn scenario_tables_empty() -> [CrdTable; 4] {
    std::array::from_fn(|_| CrdTable {
        version: 2,
        header: vec![0; 0x18],
        layers: vec![None; 8],
    })
}

#[test]
fn idle() {
    let mut m = scenario_mixer();
    let idle = 0x2400;
    for _ in 0..32 {
        m.update(&args(idle, 0), &mut Running);
    }
    let bin = m.frame().bin;
    let mut audible = 0;
    for v in 0..ENGINE_VOICES {
        let vol = m.volumes()[v];
        if vol > 0 {
            audible += 1;
            assert!(
                v < 8,
                "voice {v}: only exterior no-load (.ect) layers at idle"
            );
            assert!(m.curve_at(v, bin) > 0, "voice {v}: curve covers bin {bin}");
        }
    }
    assert!(audible > 0);
}

#[test]
fn throttle_crossfade() {
    let mut m = scenario_mixer();
    let rpm = 0x9000;
    for _ in 0..32 {
        m.update(&args(rpm, 0), &mut Running);
    }
    let before = *m.volumes();
    let mut throttle = ThrottleSmoother::default();
    let mut prev = before;
    for _ in 0..64 {
        let load = throttle.step(255) >> 1;
        m.update(&args(rpm, load), &mut Running);
        for (v, (&a, &b)) in prev.iter().zip(m.volumes()).enumerate() {
            assert!((a - b).abs() <= 8, "voice {v}: {a} -> {b}");
        }
        prev = *m.volumes();
    }
    let after = *m.volumes();
    assert_eq!(m.frame().load, 128);
    let no_load: i32 = (0..8).map(|v| after[v] - before[v]).sum();
    let load: i32 = (8..16).map(|v| after[v] - before[v]).sum();
    assert!(
        no_load < 0 && (0..8).all(|v| after[v] <= before[v]),
        "no-load layers fade out"
    );
    assert!(
        load > 0 && (8..16).all(|v| after[v] >= before[v]),
        "load layers fade in"
    );
    assert!((0..8).all(|v| after[v] == 0), "no-load silent at full load");
}

#[test]
fn throttle_release() {
    let mut t = ThrottleSmoother { value: 255 };
    let mut values = vec![255];
    while *values.last().unwrap() > 0 {
        values.push(t.step(0));
    }
    assert!(values.len() > 10, "{values:?}");
    assert!(values.windows(2).all(|w| w[1] < w[0]));
    assert_eq!(&values[..4], &[255, 224, 196, 172]);
}

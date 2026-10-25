//! Decodes an engine sound archive (`GameData/Sounds/sd*.viv`) to WAV files for listening.
//!
//! Usage: `engine_bank_wav <sd*.viv> <out-dir> [car.spl] [nfs5.exe]`
//!
//! Writes every bank patch as `<bank>_slotNNN.wav` at its own sample rate, plus two RPM
//! sweeps rendered with the original mixing rules (`0x4ac780` / `0x4ac290`):
//! `<bank>_sweep_exterior.wav` (view mode 1: `.ect`/`.elt`) and
//! `<bank>_sweep_cockpit.wav` (view mode 0: `.cct`/`.clt`).
//! Each sweep is 1 s idle, 4 s full-throttle climb to redline, 4 s lift-off fall to idle.
//!
//! Prototype only. Approximations: voice volume 0..127 is applied linearly, the mixer
//! tick rate is assumed (100 Hz), doppler and stereo spread are off. The 129-byte
//! crossfade table is read from the original executable (VA 0x5d1764).
use nfs_formats::{
    decode_patch, parse_bnk, parse_crd, parse_spl, parse_viv, viv_file, wav_bytes, BnkPatch,
    CrdTable, EngineSpl, ENGINE_TABLE_EXTS,
};
use std::path::{Path, PathBuf};

const OUT_RATE: u32 = 44_100;
/// Assumed rate of the audio-service callback that runs `0x4ac290`.
const TICK_HZ: f64 = 100.0;
/// `0x4ac290` consumes one state every third callback.
const TICKS_PER_UPDATE: u32 = 3;
const XFADE_TABLE_FILE_OFFSET: usize = 0x5d1764 - 0x400000;

struct Voice<'a> {
    index: usize,
    slot: u8,
    curve: &'a nfs_formats::CrdLayer,
    patch: &'a BnkPatch,
    pcm: Vec<i16>,
    pos: f64,
    volume: i32,
}

impl Voice<'_> {
    fn next(&mut self, ratio: f64) -> f64 {
        let n = self.pcm.len();
        let i = self.pos as usize;
        let frac = self.pos - i as f64;
        let a = self.pcm[i.min(n - 1)] as f64;
        let b = self.pcm[(i + 1).min(n - 1)] as f64;
        self.pos += self.patch.base_rate_hz() * ratio / OUT_RATE as f64;
        let (start, end) = match (self.patch.loop_start, self.patch.loop_end) {
            (Some(s), Some(e)) => (s as f64, e as f64 + 1.0),
            _ => (0.0, n as f64),
        };
        while self.pos >= end {
            self.pos -= end - start;
        }
        (a + (b - a) * frac) / 32768.0
    }
}

fn render_sweep(
    tables: &[CrdTable; 4],
    bank: &[u8],
    patches: &nfs_formats::Bnk,
    spl: &EngineSpl,
    xfade: &[u8],
    mode: u32,
) -> Vec<i16> {
    // Voices 0..15 (.ect/.elt) play when mode != 0, 16..31 (.cct/.clt) when mode == 0.
    let first_table = if mode == 0 { 2 } else { 0 };
    let mut voices = Vec::new();
    for (t, table) in tables.iter().enumerate().skip(first_table).take(2) {
        for (i, layer) in table.layers.iter().enumerate() {
            let Some(layer) = layer else { continue };
            let Some(patch) = patches.patch(layer.slot as usize) else {
                continue;
            };
            voices.push(Voice {
                index: 8 * t + i,
                slot: layer.slot,
                curve: layer,
                patch,
                pcm: decode_patch(bank, patch).unwrap(),
                pos: 0.0,
                volume: 0,
            });
        }
    }
    let t_at = |p: i32| xfade[p.clamp(0, 128) as usize] as i32;
    // Volume argument from 0x4aa050 for a listener at the car: ((127*20*0x55>>7)*1016)>>14.
    let vol_arg = (((127 * 20 * 0x55) >> 7) * 1016) >> 14;
    let master = ((spl.master_volume(mode) * vol_arg) >> 6).min(0x7ff);
    let group = spl.group_fade(mode);
    let seconds = 9.0;
    let total = (seconds * OUT_RATE as f64) as usize;
    let samples_per_tick = OUT_RATE as f64 / TICK_HZ;
    let mut throttle = 0i32;
    let mut out = Vec::with_capacity(total);
    let mut tick = 0u32;
    let mut next_tick = 0.0;
    let mut ratio = 1.0;
    for n in 0..total {
        if n as f64 >= next_tick {
            next_tick += samples_per_tick;
            let t = n as f64 / OUT_RATE as f64;
            let (rpm, pedal) = if t < 1.0 {
                (0.15, 0)
            } else if t < 5.0 {
                (0.15 + 0.85 * (t - 1.0) / 4.0, 255)
            } else {
                (1.0 - 0.85 * (t - 5.0) / 4.0, 0)
            };
            // Throttle smoothing of 0x4aa050: up by half the gap (min 1), down by 1/8 (min 1).
            let gap = pedal - throttle;
            throttle += if gap > 0 {
                (gap / 2).max(1)
            } else if gap < 0 {
                (gap / 8).min(-1)
            } else {
                0
            };
            let load = throttle >> 1;
            let p6 = (load + (load >> 6) + (load >> 5)).clamp(0, 128);
            let rpm16 = (rpm * 65536.0) as i32;
            let bin = ((spl.rpm_bin_scale() * rpm16) >> 14).min(0x1ff) as usize;
            ratio = ((spl.pitch_scale() * (rpm16 + 0x3333)) >> 10) as f64 / 4096.0;
            tick += 1;
            if tick.is_multiple_of(TICKS_PER_UPDATE) {
                for v in voices.iter_mut() {
                    let c = v.curve.volume_at(bin) as i32;
                    let load_w = if v.index % 16 < 8 {
                        t_at(128 - p6)
                    } else {
                        t_at(p6)
                    };
                    let group_w = if v.slot < 0x40 {
                        t_at(group)
                    } else {
                        t_at(128 - group)
                    };
                    let target = ((((c * load_w * group_w) >> 3) * master) >> 18).min(127);
                    v.volume = target.clamp(v.volume - 8, v.volume + 8);
                }
            }
        }
        let mut mix = 0.0;
        for v in voices.iter_mut() {
            let s = v.next(ratio);
            mix += s * v.volume as f64 / 127.0;
        }
        out.push((mix * 0.5 * 32767.0).clamp(-32768.0, 32767.0) as i16);
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: engine_bank_wav <sd*.viv> <out-dir> [car.spl] [nfs5.exe]");
        std::process::exit(2);
    }
    let viv_path = Path::new(&args[1]);
    let out_dir = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&out_dir).unwrap();
    let viv = std::fs::read(viv_path).unwrap();
    let files = parse_viv(&viv).unwrap();
    let stem = viv_path
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_lowercase();
    let bank = viv_file(&viv, &files, &format!("{stem}.bnk")).expect("bank in archive");
    let patches = parse_bnk(bank).unwrap();
    println!("slot  rate  ch  samples  loop          root fine detune  base_hz");
    for (slot, p) in patches.slots.iter().enumerate() {
        let Some(p) = p else { continue };
        let pcm = decode_patch(bank, p).unwrap();
        let path = out_dir.join(format!("{stem}_slot{slot:03}.wav"));
        std::fs::write(&path, wav_bytes(&pcm, p.channels as u16, p.sample_rate)).unwrap();
        println!(
            "{slot:4} {:5} {:3} {:8}  {:13} {:4} {:4} {:6} {:8.0}",
            p.sample_rate,
            p.channels,
            p.num_samples,
            format!("{:?}..{:?}", p.loop_start, p.loop_end),
            p.root_note(),
            p.fine_tune_cents(),
            p.detune_cents(),
            p.base_rate_hz()
        );
    }
    let tables: [CrdTable; 4] = ENGINE_TABLE_EXTS
        .map(|ext| parse_crd(viv_file(&viv, &files, &format!("{stem}.{ext}")).unwrap()).unwrap());
    let spl = args
        .get(3)
        .map(|p| parse_spl(&std::fs::read(p).unwrap()).unwrap())
        .unwrap_or(EngineSpl::DEFAULT);
    let exe = args.get(4).map(PathBuf::from).unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/nfs5.exe")
    });
    let exe = std::fs::read(&exe).unwrap_or_else(|e| panic!("{}: {e}", exe.display()));
    let xfade = &exe[XFADE_TABLE_FILE_OFFSET..XFADE_TABLE_FILE_OFFSET + 129];
    for (mode, name) in [(1, "exterior"), (0, "cockpit")] {
        let pcm = render_sweep(&tables, bank, &patches, &spl, xfade, mode);
        let path = out_dir.join(format!("{stem}_sweep_{name}.wav"));
        std::fs::write(&path, wav_bytes(&pcm, 1, OUT_RATE)).unwrap();
        println!("wrote {}", path.display());
    }
}

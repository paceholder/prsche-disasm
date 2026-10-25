//! Lists the patches of a `.bnk` (or decodes them to WAV): `bank_slots <file.bnk> [out-dir]`.
use nfs_formats::{decode_patch, parse_bnk, wav_bytes};

fn main() {
    let mut a = std::env::args().skip(1);
    let path = a.next().expect("usage: bank_slots <file.bnk> [out-dir]");
    let out = a.next();
    let data = std::fs::read(&path).unwrap();
    let bnk = parse_bnk(&data).unwrap();
    for (i, s) in bnk.slots.iter().enumerate() {
        let Some(p) = s else { continue };
        println!(
            "slot {i:3} {:?} ch{} {} Hz n={} loop={:?}..{:?} root={} fine={} tags={:?}",
            p.codec,
            p.channels,
            p.sample_rate,
            p.num_samples,
            p.loop_start,
            p.loop_end,
            p.root_note(),
            p.fine_tune_cents(),
            p.tags
        );
        if let Some(dir) = &out {
            if let Ok(pcm) = decode_patch(&data, p) {
                std::fs::create_dir_all(dir).unwrap();
                std::fs::write(
                    format!("{dir}/slot{i:03}.wav"),
                    wav_bytes(&pcm, p.channels as u16, p.sample_rate),
                )
                .unwrap();
            }
        }
    }
}

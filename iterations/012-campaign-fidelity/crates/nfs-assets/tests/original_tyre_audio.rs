//! `TyreSounds` against the original tyre-squeal and rumble chain, executed in Unicorn by
//! `scripts/research/replay-tyre-audio.py` (fixture `tests/fixtures/tyre-audio-steps.tsv`):
//! `Car_SkidSoundEvents` 0x40fbc0, `Sfx_PlayEvent` 0x4a93c0, `Sfx_Play` 0x4a9c10 and
//! `CarSound_Update` 0x4aa050, 1176 ticks over 40 cases. Per tick the events queued, the
//! `car+0xe00` bits, the voice stops (slot, fade ticks), the rpm term and, for the front, rear
//! and rumble voices, slot / volume / controller / pitch must match.
use nfs_assets::audio::{
    rpm_term, Channel, EngineView, SkidEvent, TyreCommand, TyreInputs, TyreSounds,
};
use std::collections::HashMap;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/tyre-audio-steps.tsv"
);

struct Row {
    cols: HashMap<String, String>,
}

impl Row {
    fn s(&self, k: &str) -> &str {
        &self.cols[k]
    }
    fn f(&self, k: &str) -> f32 {
        self.s(k).parse::<f64>().unwrap() as f32
    }
    fn i(&self, k: &str) -> i32 {
        self.s(k).parse().unwrap()
    }
}

fn rows() -> Vec<Row> {
    let text = std::fs::read_to_string(FIXTURE).unwrap();
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
    lines
        .map(|l| Row {
            cols: header
                .iter()
                .zip(l.split('\t'))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        })
        .collect()
}

fn inputs(r: &Row) -> TyreInputs {
    TyreInputs {
        skid_front: [r.f("f0"), r.f("f1")],
        skid_rear: [r.f("r0"), r.f("r1")],
        field_78: r.f("f78"),
        height_424: r.f("height"),
        ground_up_3a0: r.f("up"),
        surface_464: r.i("surface"),
        surface_override_540: r.i("o540") != 0,
        tyre_type_1c0: r.i("tyre"),
        flags_52c_bit4: r.i("bit4") != 0,
        vel_fwd_d60: r.f("d60"),
        rpm_db4: r.f("rpm"),
        redline_94: r.f("redline"),
        wheels_off_gt5: r.i("off") > 5,
        res_c_is_1: r.i("resc") == 1,
    }
}

fn view(camera: i32) -> EngineView {
    match camera {
        0 => EngineView::Bumper,
        2 => EngineView::Chase,
        _ => EngineView::Free,
    }
}

fn event_text(e: &SkidEvent) -> String {
    format!(
        "{}:{:#x}:{}:{}:{}",
        e.kind,
        e.channel.effect_id(),
        e.tyre,
        e.patch,
        e.volume
    )
}

#[test]
fn matches_original_ticks() {
    let rows = rows();
    assert!(rows.len() > 1000);
    let mut sounds = TyreSounds::new();
    let mut slot_of: HashMap<u32, u8> = HashMap::new();
    let mut last_case = String::new();
    for r in &rows {
        let at = format!("{} step {}", r.s("case"), r.s("step"));
        if r.s("case") != last_case {
            last_case = r.s("case").to_string();
            sounds = TyreSounds::new();
            slot_of.clear();
        }
        let car = inputs(r);
        assert_eq!(rpm_term(&car), r.i("p_term"), "{at}: rpm term");

        // Events and bits: a fresh copy of the state runs the producer alone.
        let mut probe = sounds.clone();
        let events: Vec<String> = probe.skid_events(&car).iter().map(event_text).collect();
        let want: Vec<String> = if r.s("events").is_empty() {
            Vec::new()
        } else {
            r.s("events")
                .split(';')
                .map(|e| {
                    let p: Vec<&str> = e.split(':').collect();
                    // The recorder prints the f32 with 9 digits: compare as f32.
                    format!(
                        "{}:{}:{}:{}:{}",
                        p[0],
                        p[1],
                        p[2],
                        p[3],
                        p[4].parse::<f32>().unwrap()
                    )
                })
                .collect()
        };
        assert_eq!(events, want, "{at}: events");
        assert_eq!(probe.bits_e00() as i32, r.i("bits_out"), "{at}: bits");

        let commands = sounds.tick(&car, view(r.i("camera")));
        let mut stops: Vec<String> = Vec::new();
        for c in &commands {
            match *c {
                TyreCommand::Start { voice, slot, .. } => {
                    slot_of.insert(voice, slot);
                }
                TyreCommand::Stop { voice, fade_ticks } => {
                    stops.push(format!("{}:{}", slot_of[&voice], fade_ticks));
                }
                TyreCommand::Update { .. } => {}
            }
        }
        stops.sort();
        let mut want_stops: Vec<String> = r
            .s("stops")
            .split(';')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        want_stops.sort();
        assert_eq!(stops, want_stops, "{at}: stops");

        for (name, channel) in [
            ("front", Channel::Front),
            ("rear", Channel::Rear),
            ("rumble", Channel::Rumble),
        ] {
            let got = sounds.voice(channel);
            let slot = r.i(&format!("{name}_slot"));
            match got {
                None => assert_eq!(slot, -1, "{at}: {name} should play slot {slot}"),
                Some(v) => {
                    assert_eq!(v.slot as i32, slot, "{at}: {name} slot");
                    assert_eq!(
                        v.volume as i32,
                        r.i(&format!("{name}_vol")),
                        "{at}: {name} volume"
                    );
                    assert_eq!(
                        v.control as i32,
                        r.i(&format!("{name}_ctl")),
                        "{at}: {name} control"
                    );
                    assert_eq!(
                        v.pitch as i32,
                        r.i(&format!("{name}_pitch")),
                        "{at}: {name} pitch"
                    );
                }
            }
        }
    }
}

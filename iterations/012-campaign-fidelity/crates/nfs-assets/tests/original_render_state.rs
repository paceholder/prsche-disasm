//! Material render state recorded by executing nfs5.exe `Render_ApplyLayerState` 0x540660
//! (with its setters 0x5405c0, 0x540550, 0x540590, 0x540500) and the THRASH driver
//! `Drivers/dx7/dx7z.dll` (`THRASH_setstate@8`, `THRASH_getstate@4`) in Unicorn with a
//! logging Direct3D 7 device (scripts/research/replay-render-state.py, synthetic blocks).
//! Every row must match bit for bit: THRASH calls, D3D calls, cache words, z offsets.
//!
//! Spec: car-lighting "Track uses its prelit vertex colours" (the combine); the cull rule is
//! change original-lighting task 9.13.

use std::collections::HashMap;

use nfs_assets::render_state::{
    apply_layer_state, D3dCall, DriverCaps, Dx7Driver, ThrashCall, View, CACHE_SENTINEL,
};

const RECORDING: &str = include_str!("fixtures/render-state.tsv");

fn rows() -> Vec<HashMap<&'static str, &'static str>> {
    let mut lines = RECORDING.lines().filter(|l| !l.starts_with('#'));
    let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
    lines
        .map(|l| {
            let cols: Vec<&str> = l.split('\t').collect();
            assert_eq!(cols.len(), header.len(), "{l}");
            header.iter().copied().zip(cols).collect()
        })
        .collect()
}

fn caps(mask: i32) -> DriverCaps {
    DriverCaps {
        modulate_alpha: mask & 1 != 0,
        additive_one_one: mask & 2 != 0,
        gouraud_specular: mask & 4 != 0,
        stage_ready: mask & 8 != 0,
        ..DriverCaps::default()
    }
}

fn block(text: &str) -> [i32; 6] {
    let v: Vec<i32> = text.split(',').map(|x| x.parse().unwrap()).collect();
    v.try_into().unwrap()
}

fn thrash(calls: &[ThrashCall]) -> String {
    let s: Vec<String> = calls
        .iter()
        .map(|c| match *c {
            ThrashCall::Set(id, v) => format!("set:{id:x}={v}"),
            ThrashCall::Get(id) => format!("get:{id:x}"),
        })
        .collect();
    if s.is_empty() {
        "-".into()
    } else {
        s.join(";")
    }
}

fn d3d(calls: &[D3dCall]) -> String {
    let s: Vec<String> = calls
        .iter()
        .map(|c| match *c {
            D3dCall::RenderState(s, v) => format!("rs{s}={v}"),
            D3dCall::TextureStage(st, t, v) => format!("tss{st}.{t}={v}"),
        })
        .collect();
    if s.is_empty() {
        "-".into()
    } else {
        s.join(";")
    }
}

fn bits(x: f32) -> String {
    format!("{:08x}", x.to_bits())
}

#[test]
fn render_state_matches_the_original_bit_for_bit() {
    let mut counts = HashMap::new();
    for r in rows() {
        let kind = r["kind"];
        *counts.entry(kind).or_insert(0) += 1;
        let caps = caps(r["caps"].parse().unwrap());
        let view = View {
            cull_mode: r["view_cull"].parse().unwrap(),
            z_base: r["z_base"].parse().unwrap(),
            defer_z: r["defer"] == "1",
        };
        let mut driver = Dx7Driver::new(caps);
        let label = format!(
            "{kind} prev={} block={} caps={:?} view={view:?}",
            r["prev"], r["block"], caps
        );
        match kind {
            "set" | "combine" => {
                let (id, value) = r["block"].split_once('=').unwrap();
                let id = u32::from_str_radix(id, 16).unwrap();
                let value: i32 = value.parse().unwrap();
                let ret = driver.set_state(id, value);
                assert_eq!(ret.to_string(), r["ret"], "{label}: return");
                if kind == "set" {
                    let got = driver.state(id) as u32;
                    assert_eq!(got.to_string(), r["cache"], "{label}: getstate after");
                }
            }
            "apply" | "seq" => {
                let mut cache = [CACHE_SENTINEL; 6];
                if r["prev"] != "-" {
                    apply_layer_state(&mut cache, &block(r["prev"]), &view, &mut driver);
                    driver.thrash.clear();
                    driver.d3d.clear();
                }
                let deferred =
                    apply_layer_state(&mut cache, &block(r["block"]), &view, &mut driver);
                let cache_text: Vec<String> = cache.iter().map(|v| v.to_string()).collect();
                assert_eq!(cache_text.join(","), r["cache"], "{label}: cache");
                // The deferred slot keeps its last value (0 in a fresh frame).
                assert_eq!(
                    bits(deferred.unwrap_or(0.0)),
                    r["deferred_z"],
                    "{label}: deferred z"
                );
            }
            other => panic!("unknown row kind {other}"),
        }
        assert_eq!(thrash(&driver.thrash), r["thrash"], "{label}: THRASH calls");
        assert_eq!(d3d(&driver.d3d), r["d3d"], "{label}: D3D calls");
        assert_eq!(
            bits(driver.z_offset),
            r["driver_z"],
            "{label}: driver z offset"
        );
    }
    assert_eq!(counts.get("set"), Some(&196));
    assert_eq!(counts.get("apply"), Some(&1920));
    assert!(counts.get("seq").copied().unwrap_or(0) >= 64);
    assert_eq!(counts.get("combine"), Some(&16));
}

//! AI car data (`docs/re/ai.md` section 4): `Ais_Load` 0x49ca30 with the
//! per-track tables `AI_TrackCornerFactor` 0x4a2dc0, `AI_TrackLookFactor`
//! 0x4a2950 and `AI_ModelClass` 0x4a2bc0. All name compares are ASCII
//! case-insensitive like `_stricmp` 0x5ae3c0.

/// Size of a `.ais` file / the buffer at car+0x764.
pub const AIS_SIZE: usize = 0x130;

/// Globals rewritten by every `AI_TrackCornerFactor` call (0x4a2dc0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackTables {
    /// 0x5e6e34, road width reference.
    pub width_ref_5e6e34: f32,
    /// 0x5e74ac, 10 (first, last) node ranges.
    pub ranges_5e74ac: [(i32, i32); 10],
    /// 0x5e74fc, per-range speed factor.
    pub factors_5e74fc: [f32; 10],
}

impl Default for TrackTables {
    /// Values written before the per-track branches: width 12.0 (0x41400000),
    /// ranges (0, 0), factors 1.0.
    fn default() -> Self {
        TrackTables {
            width_ref_5e6e34: 12.0,
            ranges_5e74ac: [(0, 0); 10],
            factors_5e74fc: [1.0; 10],
        }
    }
}

impl TrackTables {
    /// Store a 32-bit value at original address `addr` (0x5e6e34, 0x5e74ac..0x5e74fc
    /// ints, 0x5e74fc..0x5e7524 f32 bits).
    fn set(&mut self, addr: u32, v: u32) {
        if addr == 0x5e6e34 {
            self.width_ref_5e6e34 = f32::from_bits(v);
        } else if addr >= 0x5e74fc {
            self.factors_5e74fc[((addr - 0x5e74fc) / 4) as usize] = f32::from_bits(v);
        } else {
            let o = ((addr - 0x5e74ac) / 4) as usize;
            let r = &mut self.ranges_5e74ac[o / 2];
            if o.is_multiple_of(2) {
                r.0 = v as i32;
            } else {
                r.1 = v as i32;
            }
        }
    }
}

fn eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn any(m: &str, names: &[&str]) -> bool {
    names.iter().any(|n| eq(m, n))
}

/// `AI_ModelClass` 0x4a2bc0: car class 1..5 (track name is the global 0x657444).
pub fn model_class(model: &str, track: &str) -> i32 {
    if any(model, &["356", "356A", "356B"]) {
        return 1;
    }
    if (eq(model, "935") || eq(model, "GT2")) && eq(track, "alps") {
        return 1;
    }
    if any(model, &["550A", "928", "930T", "964"]) {
        return 2;
    }
    if any(model, &["914", "944", "959", "965T"]) {
        return 3;
    }
    if any(model, &["993", "993T", "996", "996T", "GT2", "GT3", "BOXSTER"]) {
        return 4;
    }
    5
}

/// `AI_TrackLookFactor` 0x4a2950 (multiplies sim+0x140). `reverse` is 0x657430.
pub fn track_look_factor(model: &str, track: &str, reverse: bool) -> f64 {
    // The constants are `fld dword` (f32) in the original.
    look_raw(model, track, reverse) as f32 as f64
}

fn look_raw(model: &str, track: &str, reverse: bool) -> f64 {
    let gt = any(model, &["935", "GT1", "GT2", "GT3"]);
    if !eq(track, "autobahn") {
        if eq(track, "castle") {
            if gt {
                return 0.8;
            }
            if any(model, &["993T", "996T"]) {
                return 0.85;
            }
            if any(model, &["993", "996", "BOXSTER"]) {
                return 0.9;
            }
            return 1.0;
        }
        if !eq(track, "monaco1") {
            if eq(track, "monaco2") {
                if gt {
                    return 0.85;
                }
                if eq(model, "928") {
                    return 0.925;
                }
            } else if eq(track, "monaco3") && reverse && any(model, &["930T", "965T"]) {
                return 1.1;
            }
            return 1.0;
        }
        if !reverse {
            return 1.0;
        }
    }
    // autobahn, or monaco1 reversed
    if gt {
        0.85
    } else {
        1.0
    }
}

/// `AI_TrackCornerFactor` 0x4a2dc0: returns the corner speed factor (x87 value)
/// and the globals it wrote.
pub fn track_corner_factor(model: &str, track: &str, reverse: bool) -> (f64, TrackTables) {
    // The returned constants are `fld dword` (f32) in the original.
    let (f, t) = corner_raw(model, track, reverse);
    (f as f32 as f64, t)
}

fn corner_raw(model: &str, track: &str, reverse: bool) -> (f64, TrackTables) {
    let v = model_class(model, track);
    let mut t = TrackTables::default();
    let tn = |n: &str| eq(track, n);
  let c2 = tn("monaco1");
  if c2 {
    if !reverse {
      t.set(0x5e74ac, 0x1e);
      t.set(0x5e74b0, 0x46);
      t.set(0x5e74fc, 0x3f933333);
      t.set(0x5e74b8, 0x6e);
      t.set(0x5e7500, 0x3f99999a);
      t.set(0x5e74bc, 0x1a9);
      t.set(0x5e74c0, 0x20d);
      t.set(0x5e7504, 0x3f933333);
      t.set(0x5e74c4, 0x20d);
      t.set(0x5e7508, 0x3f90a3d7);
      t.set(0x5e74cc, 200);
      t.set(0x5e74d0, 300);
      t.set(0x5e750c, 0x3f333333);
    }
    else {
      t.set(0x5e74ac, 0x14);
      t.set(0x5e74b0, 0x32);
      t.set(0x5e74fc, 0x3f8ccccd);
      t.set(0x5e74b8, 0x82);
      t.set(0x5e7500, 0x3f8ccccd);
      t.set(0x5e74c4, 0x186);
      t.set(0x5e7508, 0x3f933333);
      t.set(0x5e74cc, 0x8b);
      t.set(0x5e74d0, 0x96);
      t.set(0x5e750c, 0x41c80000);
      t.set(0x5e74d4, 0x244);
      t.set(0x5e74d8, 0x250);
      t.set(0x5e7510, 0x41f00000);
      t.set(0x5e74dc, 200);
      t.set(0x5e74e0, 300);
      t.set(0x5e7514, 0x3f333333);
    }
    t.set(0x5e74c8, 0x22d);
    t.set(0x5e74b4, 0x5a);
    if v == 5 {
      return (1.5, t);
    }
    if v == 4 {
      return (1.325, t);
    }
    if v == 3 {
      return (1.275, t);
    }
    if v == 2 {
      return (1.22, t);
    }
    return (1.175, t);
  }
  let c2 = tn("monaco2");
  if c2 {
    let f5: f64 = if v == 5 {
      1.6
    }
    else if v == 4 {
      1.325
    }
    else if v == 3 {
      1.275
    }
    else if v == 2 {
      1.2
    }
    else {
      1.175
    };
    if !reverse {
      t.set(0x5e74ac, 0x1e0);
      t.set(0x5e74b0, 0x261);
      t.set(0x5e74fc, 0x3f900000);
      t.set(0x5e74b4, 0xff);
      t.set(0x5e74b8, 0x113);
      t.set(0x5e7500, 0x3f900000);
      t.set(0x5e74bc, 0x50);
      t.set(0x5e74c0, 0x78);
      t.set(0x5e7504, 0x3f4ccccd);
      return (f5, t);
    }
    t.set(0x5e74ac, 0);
    t.set(0x5e74b0, 0x46);
    t.set(0x5e74fc, 0x3dcccccd);
    t.set(0x5e74b4, 0x46);
    t.set(0x5e74b8, 0x8c);
    t.set(0x5e7500, 0x3f333333);
    t.set(0x5e74c4, 0x10b);
    t.set(0x5e74c8, 0x15e);
    t.set(0x5e7508, 0x3fd9999a);
    t.set(0x5e74cc, 0x1e0);
    t.set(0x5e74d0, 0x261);
    t.set(0x5e750c, 0x3f866666);
    return (f5, t);
  }
  let c2 = tn("monaco3");
  if c2 {
    if reverse {
      t.set(0x5e74ac, 0xb9);
      t.set(0x5e74b0, 200);
      t.set(0x5e74fc, 0x3f8ccccd);
      t.set(0x5e74b4, 0x154);
      t.set(0x5e74b8, 0x19f);
      t.set(0x5e7500, 0x3f933333);
      t.set(0x5e74bc, 0x1fa);
      t.set(0x5e74c0, 0x212);
      t.set(0x5e7504, 0x3fa00000);
      t.set(0x5e74c4, 200);
      t.set(0x5e74c8, 300);
      t.set(0x5e7508, 0x3f933333);
      t.set(0x5e74cc, 0x82);
      t.set(0x5e74d0, 0xaa);
      t.set(0x5e750c, 0x3f8ccccd);
      t.set(0x5e74d4, 7);
      t.set(0x5e74d8, 0x2d);
      t.set(0x5e7510, 0x3fb33333);
      t.set(0x5e74dc, 0x2ab);
      t.set(0x5e74e0, 0x2b8);
      t.set(0x5e7514, 0x3fc00000);
      t.set(0x5e74e4, 0x1ea);
      t.set(0x5e74e8, 0x1fb);
      t.set(0x5e7518, 0x3fa00000);
      t.set(0x5e74ec, 0x46);
      t.set(0x5e74f0, 0x5a);
      t.set(0x5e751c, 0x3f933333);
      if v == 5 {
        return (1.6, t);
      }
      if v == 4 {
        return (1.325, t);
      }
      if v == 3 {
        return (1.2, t);
      }
      return (1.15, t);
    }
    t.set(0x5e74ac, 0x41);
    t.set(0x5e74b0, 0x91);
    t.set(0x5e74fc, 0x3f8ccccd);
    t.set(0x5e74b4, 0x1eb);
    t.set(0x5e74b8, 0x1f9);
    t.set(0x5e7500, 0x3f933333);
    t.set(0x5e74bc, 0x23a);
    t.set(0x5e74c0, 0x273);
    t.set(0x5e7504, 0x3f933333);
    t.set(0x5e74c4, 0xe1);
    t.set(0x5e74c8, 0x118);
    t.set(0x5e7508, 0x3f4ccccd);
    if v == 5 {
      return (1.6, t);
    }
    if v == 4 {
      return (1.325, t);
    }
    if v == 3 {
      return (1.275, t);
    }
    if v == 2 {
      return (1.225, t);
    }
    return (1.2, t);
  }
  let c2 = tn("monaco4");
  if c2 {
    if !reverse {
      t.set(0x5e74ac, 600);
      t.set(0x5e74b0, 0x2b8);
      t.set(0x5e74fc, 0x3fb33333);
      t.set(0x5e74b4, 300);
      t.set(0x5e74b8, 0x139);
      t.set(0x5e7500, 0x3fa00000);
      t.set(0x5e74c0, 0x7d);
      t.set(0x5e74c4, 0x2f8);
      t.set(0x5e74c8, 0x343);
      t.set(0x5e7508, 0x3dcccccd);
      t.set(0x5e74cc, 0x104);
      t.set(0x5e74d0, 0x113);
      t.set(0x5e74d4, 0x112);
      t.set(0x5e74d8, 300);
      t.set(0x5e7510, 0x3fe66666);
      t.set(0x5e74dc, 0x195);
      t.set(0x5e74e0, 0x1d6);
      t.set(0x5e7514, 0x3f933333);
      t.set(0x5e74e4, 0x3c);
      t.set(0x5e74e8, 0x72);
      t.set(0x5e7518, 0x3f8ccccd);
    }
    else {
      t.set(0x5e74b0, 0xeb);
      t.set(0x5e74ac, 0x88);
      t.set(0x5e74c0, 0x88);
      t.set(0x5e74fc, 0x3f99999a);
      t.set(0x5e74b4, 0x12d);
      t.set(0x5e74b8, 0x13a);
      t.set(0x5e7500, 0x3f99999a);
      t.set(0x5e74c4, 0x139);
      t.set(0x5e74c8, 0x163);
      t.set(0x5e7508, 0x3fa00000);
      t.set(0x5e74cc, 0x122);
      t.set(0x5e74d0, 0x12e);
      t.set(0x5e74d4, 0x115);
      t.set(0x5e74d8, 0x123);
      t.set(0x5e7510, 0x3fc00000);
      t.set(0x5e74dc, 0x1e0);
      t.set(0x5e74e0, 0x1fe);
      t.set(0x5e7514, 0x3dcccccd);
      t.set(0x5e74e4, 0x2b7);
      t.set(0x5e74e8, 0x2ee);
      t.set(0x5e7518, 0x3f99999a);
      t.set(0x5e74ec, 0x1b8);
      t.set(0x5e74f0, 0x1e0);
      t.set(0x5e751c, 0x3f99999a);
      t.set(0x5e74f4, 5);
      t.set(0x5e74f8, 0x50);
      t.set(0x5e7520, 0x3f933333);
    }
    t.set(0x5e750c, 0x3fa66666);
    t.set(0x5e7504, 0x3dcccccd);
    t.set(0x5e74bc, 0x70);
    if v == 5 {
      return (1.375, t);
    }
    if v == 4 {
      return (1.325, t);
    }
    if v == 3 {
      return (1.275, t);
    }
    if v == 2 {
      return (1.2, t);
    }
    if !reverse {
      t.set(0x5e74d4, 0x112);
      t.set(0x5e74d8, 300);
      t.set(0x5e7510, 0x3fb33333);
    }
    return (1.2, t);
  }
  let c2 = tn("monaco5");
  if c2 {
    if !reverse {
      t.set(0x5e74b0, 300);
      t.set(0x5e74ac, 0x116);
      t.set(0x5e74b4, 0x24e);
      t.set(0x5e74b8, 0x261);
      t.set(0x5e74bc, 0x212);
      t.set(0x5e74c0, 600);
      t.set(0x5e7504, 0x3f333333);
      t.set(0x5e74c4, 0xfa);
      t.set(0x5e74c8, 0x116);
      t.set(0x5e7508, 0x3f8ccccd);
      t.set(0x5e74cc, 400);
      t.set(0x5e74d0, 0x1d6);
      t.set(0x5e750c, 0x3fa00000);
    }
    else {
      t.set(0x5e74ac, 0);
      t.set(0x5e74b0, 0x14);
      t.set(0x5e74b4, 0x116);
      t.set(0x5e74b8, 0x12f);
      t.set(0x5e74bc, 0x1c7);
      t.set(0x5e74c0, 0x1eb);
      t.set(0x5e7504, 0x3f933333);
      t.set(0x5e74c4, 0x1ea);
      t.set(0x5e74c8, 0x208);
      t.set(0x5e7508, 0x3fa66666);
      t.set(0x5e74cc, 0x3c);
      t.set(0x5e74d0, 0x8c);
      t.set(0x5e750c, 0x3f59999a);
    }
    t.set(0x5e7500, 0x3dcccccd);
    t.set(0x5e74fc, 0x3dcccccd);
    if v == 5 {
      return (1.5, t);
    }
    if v == 4 {
      return (1.325, t);
    }
    if v == 3 {
      return (1.275, t);
    }
    return (1.2, t);
  }
  let c2 = tn("alps");
  if c2 {
    t.set(0x5e74ac, 300);
    if !reverse {
      t.set(0x5e74b0, 0x14a);
      t.set(0x5e74fc, 0x3fcccccd);
      t.set(0x5e74b4, 0x15e);
      t.set(0x5e74b8, 400);
      t.set(0x5e7500, 0x3f99999a);
      t.set(0x5e74bc, 0x2ee);
      t.set(0x5e74c0, 0x348);
    }
    else {
      t.set(0x5e74b0, 0x1c2);
      t.set(0x5e74fc, 0x3fb33333);
      t.set(0x5e74bc, 800);
      t.set(0x5e74c0, 900);
    }
    t.set(0x5e7504, 0x3fc00000);
    t.set(0x5e74c4, 900);
    t.set(0x5e74c8, 0x546);
    t.set(0x5e7508, 0x3f59999a);
    t.set(0x5e74cc, 0x5dc);
    t.set(0x5e74d0, 0x6a4);
    t.set(0x5e750c, 0x3f333333);
    if v == 5 {
      return (1.3, t);
    }
    if 2 < v {
      return (1.2, t);
    }
    if v == 2 {
      return (1.15, t);
    }
// LAB_004a45c5:
    return (1.0, t);
  }
  let c2 = tn("autobahn");
  if c2 {
    if !reverse {
      t.set(0x5e74ac, 0x23f);
      t.set(0x5e74b0, 0x25d);
      t.set(0x5e74fc, 0x3f99999a);
      t.set(0x5e7508, 0x3fa66666);
      t.set(0x5e74d4, 0x587);
      t.set(0x5e74d8, 0x5b9);
      t.set(0x5e7510, 0x3f8ccccd);
      t.set(0x5e74dc, 0x712);
      t.set(0x5e74e0, 0x742);
      t.set(0x5e7514, 0x3f8ccccd);
      t.set(0x5e74e4, 0xabe);
      t.set(0x5e74e8, 0xae3);
    }
    else {
      t.set(0x5e74ac, 0x235);
      t.set(0x5e74b0, 0x262);
      t.set(0x5e74fc, 0x3fc00000);
      t.set(0x5e7508, 0x3f8ccccd);
      t.set(0x5e74d4, 0x1a4);
      t.set(0x5e74d8, 0x1d6);
      t.set(0x5e7510, 0x3f99999a);
      t.set(0x5e74dc, 0x2c1);
      t.set(0x5e74e0, 0x2e9);
      t.set(0x5e7514, 0x3f99999a);
      t.set(0x5e74e4, 0x741);
      t.set(0x5e74e8, 0x771);
      t.set(0x5e74ec, 0x3aa);
      t.set(0x5e74f0, 0x3c5);
      t.set(0x5e751c, 0x3fc00000);
      t.set(0x5e74f4, 0x582);
      t.set(0x5e74f8, 0x5b4);
      t.set(0x5e7520, 0x3f99999a);
    }
    t.set(0x5e7518, 0x3f8ccccd);
    t.set(0x5e750c, 0x3fa66666);
    t.set(0x5e7504, 0x3f99999a);
    t.set(0x5e7500, 0x3f99999a);
    t.set(0x5e74d0, 0x5f0);
    t.set(0x5e74cc, 0x5be);
    t.set(0x5e74c8, 0x357);
    t.set(0x5e74c4, 0x325);
    t.set(0x5e74c0, 0x2b7);
    t.set(0x5e74bc, 0x294);
    t.set(0x5e74b8, 0x398);
    t.set(0x5e74b4, 0x36b);
    if v == 5 {
      return (1.3, t);
    }
    if v == 4 {
      return (1.2, t);
    }
    if 1 < v {
      return (1.1, t);
    }
  }
  else {
    let c2 = tn("canyon");
    if c2 {
      if !reverse {
        t.set(0x5e74ac, 400);
        t.set(0x5e74b0, 500);
        t.set(0x5e74b4, 0x3a2);
        t.set(0x5e74b8, 0x406);
        t.set(0x5e7500, 0x3f8ccccd);
        t.set(0x5e74bc, 0x438);
        t.set(0x5e74c0, 0x46a);
        t.set(0x5e74c4, 0x492);
        t.set(0x5e74c8, 0x4f6);
        t.set(0x5e7508, 0x3fa00000);
        t.set(0x5e74cc, 0x640);
        t.set(0x5e74d0, 0x686);
        t.set(0x5e750c, 0x3f8ccccd);
        t.set(0x5e74d4, 0x37);
        t.set(0x5e74d8, 0x172);
        t.set(0x5e7510, 0x3f733333);
      }
      else {
        t.set(0x5e74ac, 0x2da);
        t.set(0x5e74b0, 0x33e);
        t.set(0x5e74b4, 0x492);
        t.set(0x5e74b8, 0x4f6);
        t.set(0x5e7500, 0x3fa00000);
        t.set(0x5e74bc, 0x640);
        t.set(0x5e74c0, 0x686);
        t.set(0x5e74c4, 0x690);
        t.set(0x5e74c8, 0x6c2);
        t.set(0x5e7508, 0x3f8ccccd);
        t.set(0x5e74cc, 0x37);
        t.set(0x5e74d0, 0xfa);
        t.set(0x5e750c, 0x3f59999a);
      }
      t.set(0x5e7504, 0x3f8ccccd);
      t.set(0x5e74fc, 0x3f933333);
      if v == 5 {
        return (1.2, t);
      }
      if 2 < v {
        return (1.15, t);
      }
// LAB_004a4160:
      return (1.1, t);
    }
    let c2 = tn("castle");
    if c2 {
      t.set(0x5e74ac, 0x32);
      t.set(0x5e74b0, 0xaf);
      if !reverse {
        t.set(0x5e74fc, 0x3f733333);
        t.set(0x5e74c0, 0xb7c);
        t.set(0x5e74f4, 0xd64);
        t.set(0x5e74f8, 0xd72);
        t.set(0x5e7520, 0x3fa66666);
      }
      else {
        t.set(0x5e74fc, 0x3f666666);
        t.set(0x5e74c0, 0xab4);
      }
      t.set(0x5e751c, 0x3fa66666);
      t.set(0x5e7518, 0x3f666666);
      t.set(0x5e7514, 0x3f59999a);
      t.set(0x5e7510, 0x3fb33333);
      t.set(0x5e750c, 0x3fc00000);
      t.set(0x5e7508, 0x3fa66666);
      t.set(0x5e7504, 0x3f666666);
      t.set(0x5e7500, 0x3fc00000);
      t.set(0x5e74f0, 0x3fc);
      t.set(0x5e74ec, 1000);
      t.set(0x5e74e8, 1000);
      t.set(0x5e74e4, 0x17c);
      t.set(0x5e74e0, 0xca8);
      t.set(0x5e74dc, 0xc30);
      t.set(0x5e74d8, 0xd3e);
      t.set(0x5e74d4, 0xd34);
      t.set(0x5e74d0, 0xcfd);
      t.set(0x5e74cc, 0xcf1);
      t.set(0x5e74c8, 3000);
      t.set(0x5e74c4, 0xb90);
      t.set(0x5e74bc, 0x965);
      t.set(0x5e74b8, 0x136);
      t.set(0x5e74b4, 0x122);
      t.set(0x5e6e34, 0x41200000);
      if 3 < v {
        return (1.1, t);
      }
    }
    else {
      let c2 = tn("coastal");
      if c2 {
        t.set(0x5e74ac, 0x10e);
        if !reverse {
          t.set(0x5e74b0, 0x1b8);
          t.set(0x5e74fc, 0x3f8ccccd);
          t.set(0x5e74b4, 0x21c);
          t.set(0x5e74b8, 0x230);
          t.set(0x5e7500, 0x3f866666);
          t.set(0x5e74bc, 0x276);
          t.set(0x5e74c0, 800);
          t.set(0x5e7504, 0x3f933333);
          t.set(0x5e74c4, 0x3b6);
          t.set(0x5e74c8, 0x3ca);
          t.set(0x5e7508, 0x3f8ccccd);
          t.set(0x5e74cc, 0x3ca);
          t.set(0x5e74d0, 0x41a);
          t.set(0x5e750c, 0x40000000);
        }
        else {
          t.set(0x5e74b0, 0x140);
          t.set(0x5e74fc, 0x3f933333);
          t.set(0x5e74b4, 0x1ae);
          t.set(0x5e74b8, 500);
          t.set(0x5e7500, 0x3fa00000);
          t.set(0x5e74bc, 0x208);
          t.set(0x5e74c0, 0x230);
          t.set(0x5e7504, 0x3f866666);
          t.set(0x5e74c4, 0x294);
          t.set(0x5e74c8, 800);
          t.set(0x5e7508, 0x3f99999a);
          t.set(0x5e74cc, 0x334);
          t.set(0x5e74d0, 0x366);
          t.set(0x5e750c, 0x3fc00000);
        }
        t.set(0x5e74d8, 0x41a);
        t.set(0x5e74d4, 0x3ca);
        t.set(0x5e7510, 0x3f99999a);
        t.set(0x5e74dc, 0x578);
        t.set(0x5e74e0, 0x5b6);
        t.set(0x5e7514, 0x3f99999a);
        if v == 5 {
          return (1.3, t);
        }
        if 2 < v {
          return (1.2, t);
        }
        return (1.1, t);
      }
      let c2 = tn("farmland");
      if !c2 {
        let c2 = tn("foothills");
        if !c2 {
          let c2 = tn("forest");
          if c2 {
            t.set(0x5e74ac, 0x32);
            t.set(0x5e74b0, 0x96);
            t.set(0x5e74fc, 0x3f4ccccd);
            t.set(0x5e74b4, 200);
            t.set(0x5e74b8, 0x118);
            t.set(0x5e7500, 0x3f666666);
            if !reverse {
              t.set(0x5e74c4, 0x334);
              t.set(0x5e74c8, 0x41a);
              t.set(0x5e74cc, 0x442);
              t.set(0x5e74d0, 0x460);
              t.set(0x5e750c, 0x3f933333);
              t.set(0x5e74d4, 0x4e2);
              t.set(0x5e74d8, 0x4f6);
              t.set(0x5e7510, 0x3f8ccccd);
              t.set(0x5e74dc, 0x546);
              t.set(0x5e74e0, 0x5be);
              t.set(0x5e7514, 0x3f333333);
              t.set(0x5e74e4, 0x5be);
              t.set(0x5e74e8, 0x6d6);
              t.set(0x5e74ec, 0x73a);
              t.set(0x5e74f0, 0x799);
              t.set(0x5e751c, 0x3ecccccd);
            }
            else {
              t.set(0x5e74c4, 0x352);
              t.set(0x5e74c8, 900);
              t.set(0x5e74cc, 0x3b6);
              t.set(0x5e74d0, 0x41a);
              t.set(0x5e750c, 0x3f666666);
              t.set(0x5e74d4, 0x44c);
              t.set(0x5e74d8, 0x47e);
              t.set(0x5e7510, 0x3f933333);
              t.set(0x5e74dc, 0x50a);
              t.set(0x5e74e0, 0x546);
              t.set(0x5e7514, 0x3fc00000);
              t.set(0x5e74e4, 0x546);
              t.set(0x5e74e8, 0x5aa);
              t.set(0x5e74ec, 0x5f0);
              t.set(0x5e74f0, 0x6d6);
              t.set(0x5e751c, 0x3f666666);
              t.set(0x5e74f4, 0x73a);
              t.set(0x5e74f8, 0x799);
              t.set(0x5e7520, 0x3ecccccd);
            }
            t.set(0x5e7518, 0x3f666666);
            t.set(0x5e7508, 0x3f666666);
            if v == 5 {
              t.set(0x5e6e34, 0x41100000);
              return (1.25, t);
            }
            if v == 4 {
              t.set(0x5e6e34, 0x41100000);
              return (1.15, t);
            }
            if v == 3 {
              t.set(0x5e6e34, 0x41100000);
              return (1.05, t);
            }
            t.set(0x5e6e34, 0x41100000);
            return (1.0, t);
          }
          let c2 = tn("industrial");
          if !c2 { return (1.0, t); }
          if !reverse {
            t.set(0x5e74ac, 0x334);
            t.set(0x5e74b0, 0x370);
            t.set(0x5e74fc, 0x3f19999a);
            t.set(0x5e74b4, 0x3de);
            t.set(0x5e74b8, 0x578);
            t.set(0x5e7500, 0x3f400000);
            t.set(0x5e74bc, 0x604);
            t.set(0x5e74c0, 0x640);
            t.set(0x5e7504, 0x3faccccd);
          }
          else {
            t.set(0x5e74ac, 0x262);
            t.set(0x5e74b0, 0x2a8);
            t.set(0x5e74fc, 0x3fa66666);
            t.set(0x5e74b4, 0x334);
            t.set(0x5e74b8, 0x370);
            t.set(0x5e7500, 0x3f19999a);
            t.set(0x5e74bc, 0x3de);
            t.set(0x5e74c0, 0x578);
            t.set(0x5e7504, 0x3f400000);
          }
          if v == 5 {
            return (1.1, t);
          }
          return (1.0, t);
        }
        if !reverse {
          t.set(0x5e74ac, 0x41a);
          t.set(0x5e74b0, 0x44c);
          t.set(0x5e74b4, 0x49c);
          t.set(0x5e74b8, 0x528);
          t.set(0x5e74bc, 0x366);
          t.set(0x5e74c0, 0x398);
          t.set(0x5e7504, 0x3f333333);
        }
        else {
          t.set(0x5e74ac, 0x26c);
          t.set(0x5e74b0, 600);
          t.set(0x5e74b4, 0x2a8);
          t.set(0x5e74b8, 0x2d0);
          t.set(0x5e74bc, 0x4d8);
          t.set(0x5e74c0, 0x528);
          t.set(0x5e7504, 0x3f8ccccd);
          t.set(0x5e74c4, 0x366);
          t.set(0x5e74c8, 0x3ca);
          t.set(0x5e7508, 0x3f333333);
        }
        t.set(0x5e7500, 0x3f99999a);
        t.set(0x5e74fc, 0x3f99999a);
        if v == 5 {
          return (1.4, t);
        }
        if v == 4 {
          return (1.3, t);
        }
        if v == 3 {
          return (1.25, t);
        }
// LAB_004a414d:
        return (1.2, t);
      }
      if !reverse {
        t.set(0x5e74ac, 800);
        t.set(0x5e74b0, 0x4f6);
        t.set(0x5e74fc, 0x3f59999a);
        t.set(0x5e74b4, 0x514);
        t.set(0x5e74bc, 0x726);
        t.set(0x5e74c0, 0x73f);
        t.set(0x5e7504, 0x3f933333);
      }
      else {
        t.set(0x5e74ac, 0x172);
        t.set(0x5e74b0, 0x226);
        t.set(0x5e74fc, 0x3f99999a);
        t.set(0x5e74b4, 0x3ca);
      }
      t.set(0x5e7500, 0x3f59999a);
      t.set(0x5e74b8, 0x5aa);
      if v == 5 { return (1.2, t); }
      if v == 4 { return (1.1, t); }
    }
    if v == 3 {
      return (1.05, t);
    }
  }
// LAB_004a45bb:
  (1.0, t)
}

/// One `.ais` buffer (0x130 bytes, as at car+0x764).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AisData {
    pub raw: [u8; AIS_SIZE],
}

impl AisData {
    fn f32_at(&self, o: usize) -> f32 {
        f32::from_le_bytes(self.raw[o..o + 4].try_into().unwrap())
    }

    /// Look-ahead value i (24 floats at +0x20).
    pub fn lookahead_20(&self, i: usize) -> f32 {
        assert!(i < 24);
        self.f32_at(0x20 + 4 * i)
    }

    /// Corner speed i (40 floats at +0x80).
    pub fn corner_speed_80(&self, i: usize) -> f32 {
        assert!(i < 40);
        self.f32_at(0x80 + 4 * i)
    }
}

/// `Ais_Load` 0x49ca30 on the bytes of `<model>.ais`: scales the corner speeds
/// toward their minimum by the grip ratio (car+0x10d8), multiplies by the track
/// corner factor, and scales `sim_140` (sim+0x140) by the look factor. Returns
/// the buffer copied to car+0x764 and the globals of 0x4a2dc0 (from the last
/// of its 40 calls; all are identical).
pub fn load_ais(
    file: &[u8],
    model: &str,
    track: &str,
    reverse: bool,
    grip_ratio_10d8: f32,
    sim_140: &mut f32,
) -> (AisData, TrackTables) {
    let mut raw = [0u8; AIS_SIZE];
    let n = file.len().min(AIS_SIZE);
    raw[..n].copy_from_slice(&file[..n]);
    let mut d = AisData { raw };
    let mut m = 100.0f32;
    for i in (0..40).rev() {
        let x = d.corner_speed_80(i);
        if x < m {
            m = x;
        }
    }
    // Each step: x87 extended math ((v - m) * mu + m), stored as f32 (0x49cad1..0x49cae1),
    // then times the factor in ST0, stored as f32 (0x49caf5..0x49cafe).
    let mut tables = TrackTables::default();
    for i in 0..40 {
        let v = d.corner_speed_80(i) as f64;
        let s = (((v - m as f64) * grip_ratio_10d8 as f64) + m as f64) as f32;
        let (f, t) = track_corner_factor(model, track, reverse);
        tables = t;
        let r = (f * s as f64) as f32;
        d.raw[0x80 + 4 * i..0x84 + 4 * i].copy_from_slice(&r.to_le_bytes());
    }
    let lf = track_look_factor(model, track, reverse);
    *sim_140 = (lf * *sim_140 as f64) as f32;
    (d, tables)
}

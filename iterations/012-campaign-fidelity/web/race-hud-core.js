// Original race HUD: pure logic (no DOM), shared by race-hud.js and race-hud.test.mjs.
// Semantics and evidence: docs/re/hud.md in the workspace root.

export const HUD1_RECORD = 0x1e0; // FEData/Data/nfs5-1.hud, one record per config [VA 0x41f7a0]
export const HUD2_RECORD = 0x68; // FEData/Data/nfs5-2.hud, per config and resolution [VA 0x41f7a0]
export const DESIGN_W = 640; // the fractions are converted through 640x480 [VA 0x41eff0]
export const DESIGN_H = 480;

// Position slots: pixel position global <- fraction pair, size pair [VA 0x41fb40, 0x41eff0].
// `rec` 1 = hud1 record, 2 = hud2 record.
export const SLOTS = [
  { rec: 1, frac: 0x188, size: 0x94 }, // S0  0x60c828
  { rec: 1, frac: 0x190, size: 0x9c }, // S1  0x60c830
  { rec: 1, frac: 0x198, size: 0xa4 }, // S2  0x60c838
  { rec: 1, frac: 0x1a0, size: 0xac }, // S3  0x60c840
  { rec: 1, frac: 0x1a8, size: 0xb4 }, // S4  0x60c848
  { rec: 1, frac: 0x1b0, size: 0xbc }, // S5  0x60c850
  { rec: 1, frac: 0x1b8, size: 0xc4 }, // S6  0x60c858 tachometer
  { rec: 1, frac: 0x1c0, size: 0xd4 }, // S7  0x60c860 speedometer
  { rec: 1, frac: 0x1c8, size: 0xe4 }, // S8  0x60c868 track map
  { rec: 1, frac: 0x1d0, size: 0xf4 }, // S9  0x60c870
  { rec: 1, frac: 0x1d8, size: 0xfc }, // S10 0x60c878
  { rec: 2, frac: 0x0c, size: 0x00 }, //  S11 0x60c880
  { rec: 2, frac: 0x20, size: 0x14 }, //  S12 0x60c888
];
export const SLOT = { TACH: 6, SPEEDO: 7, MAP: 8 };

// hud2 +0x30..+0x64: the display options, in the order of the race-config keys 0x5c..0x69
// (table 0x5d2070: {id, variable, name}) [VA 0x4b14ce.., 0x41e940, 0x41e960].
export const OPTION_NAMES = [
  "speedometer", "speedUnits", "tachometer", "damageDisplay", "map", "mapScale", "opponentList",
  "splitTimes", "displayColors", "driverSnapshot", "translucency", "rearViewMirror", "recordTime", "gear",
];

const cstr = (bytes, off, max) => {
  let s = "";
  for (let i = 0; i < max && bytes[off + i]; i++) s += String.fromCharCode(bytes[off + i]);
  return s;
};

// Both .hud files -> [{name, hud1: DataView, hud2: DataView, width, height}] [VA 0x41f7a0].
export function parseHudFiles(hud1Bytes, hud2Bytes) {
  const count = Math.floor(hud1Bytes.length / HUD1_RECORD);
  const records = [];
  for (let i = 0; i < count; i++) {
    const o1 = i * HUD1_RECORD;
    const o2 = i * HUD2_RECORD;
    if (o2 + HUD2_RECORD > hud2Bytes.length) break;
    const hud1 = new DataView(hud1Bytes.buffer, hud1Bytes.byteOffset + o1, HUD1_RECORD);
    const hud2 = new DataView(hud2Bytes.buffer, hud2Bytes.byteOffset + o2, HUD2_RECORD);
    records.push({
      name: cstr(hud1Bytes, o1, 16), hud1, hud2,
      width: hud2.getInt32(0x28, true), height: hud2.getInt32(0x2c, true),
    });
  }
  return records;
}

// Screen-quadrant anchoring of a 640x480 position [VA 0x41e480]: on screens wider than 640 a box
// across x = 320 is centred, boxes right of 320 move right by W - 640, boxes below y = 240 move
// down by H - 480 (only for boxes right of 320 when y <= 240, see the branch structure); then
// the box is kept on screen.
export function anchorBox(x, y, w, h, W, H) {
  if (W > 640) {
    if (x < 320 && x + w > 320) {
      x = Math.trunc(W / 2) - Math.trunc(w / 2);
      if (y > 240) y = H - 480 + y;
    } else if (x <= 320) {
      if (y > 240) y = H - 480 + y;
    } else if (y <= 240) {
      x = W - 640 + x;
    } else {
      x = W - 640 + x;
      y = H - 480 + y;
    }
  }
  if (W < x + w) x = W - w;
  if (H < h + y) y = H - h;
  return [x, y];
}

const f32 = Math.fround;

// The config used for a screen [VA 0x41f7a0, 0x41ebf0, 0x41eff0, 0x41fb40]: a record whose hud2
// resolution matches and whose name is "Config <index+1>"; otherwise record `index` converted
// to the screen (fraction * 640x480, anchored, divided by the screen size).
// `overrides`: display options edited on the options screen or by the race keys (they live in
// the hud2 record in memory and are written back to nfs5-2.hud by 0x41ea50; A85).
export function selectConfig(records, index, W, H, overrides = null) {
  const name = `Config ${index + 1}`;
  const match = records.find(r => r.width === W && r.height === H && r.name === name);
  const base = match ?? records[index] ?? records[0];
  if (!base) return null;
  const sizes = [];
  const fracs = [];
  for (const s of SLOTS) {
    const view = s.rec === 1 ? base.hud1 : base.hud2;
    const w = view.getInt32(s.size, true);
    const h = view.getInt32(s.size + 4, true);
    let fx = view.getFloat32(s.frac, true);
    let fy = view.getFloat32(s.frac + 4, true);
    if (!match) {
      const [x, y] = anchorBox(Math.trunc(f32(fx * DESIGN_W)), Math.trunc(f32(fy * DESIGN_H)), w, h, W, H);
      fx = f32(x / W);
      fy = f32(y / H);
    }
    sizes.push([w, h]);
    fracs.push([fx, fy]);
  }
  const options = {};
  OPTION_NAMES.forEach((key, i) => { options[key] = base.hud2.getInt32(0x30 + 4 * i, true); });
  if (overrides) for (const key of OPTION_NAMES) if (Number.isInteger(overrides[key])) options[key] = overrides[key];
  const hud1 = base.hud1;
  return {
    name, matched: Boolean(match), hud1, hud2: base.hud2, options,
    // Pixel boxes [VA 0x41fb40]: position = ftol(fraction * screen size).
    slots: fracs.map(([fx, fy], i) => ({
      x: Math.trunc(f32(fx * W)), y: Math.trunc(f32(fy * H)), w: sizes[i][0], h: sizes[i][1],
    })),
    // +0x14: high byte = alpha of the dark backings (0: none). Hud_Build sets it from TRANSLUCENCY:
    // 0 ("on") -> 0x80 [0x5ccb34], else 0 [VA 0x424460, 0x41e540, 0x41e590, 0x420f30]; F8 toggles it.
    translucency: translucencyAlpha(options.translucency),
    fractions: fracs,
    colors: [0x18, 0x1c, 0x20, 0x24, 0x28, 0x2c, 0x30].map(o => hud1.getUint32(o, true)),
    fonts: [cstr(new Uint8Array(hud1.buffer, hud1.byteOffset), 0x4c, 24),
      cstr(new Uint8Array(hud1.buffer, hud1.byteOffset), 0x64, 24),
      cstr(new Uint8Array(hud1.buffer, hud1.byteOffset), 0x7c, 24)],
  };
}

// Alpha of the dark backings for a TRANSLUCENCY value (0 = "on") [VA 0x420f30, 0x5ccb34 = 0x80].
export const BACKING_ALPHA = 0x80;
export function translucencyAlpha(translucency) {
  return translucency === 0 ? BACKING_ALPHA : 0;
}

// --- Display options screen (FEData/Layouts/display.lay, "Anzeige") ---------------------------
// Accessor `Display.<name>` -> race-config key and its choice strings (festrings ids), in value
// order [VA 0x4e4f70: FUN_004bb680(name, key, list); lists 0x5b6c48..0x5b6d18]. `option` is the
// hud2 field (OPTION_NAMES) the screen writes back into the selected config's record when it
// closes [VA 0x4cb860 -> setters 0x420e10..0x421010, 0x41ea50 rewrites nfs5-2.hud].
export const DISPLAY_OPTIONS = [
  { accessor: "Display.ScreenLayout", key: 0x5b, option: null, strings: [1824, 1825, 1826, 1827] },
  { accessor: "Display.Speedometer", key: 0x5c, option: "speedometer", strings: [1500, 1801, 1802] },
  { accessor: "Display.SpeedUnits", key: 0x5d, option: "speedUnits", strings: [1803, 1804] },
  { accessor: "Display.Tach", key: 0x5e, option: "tachometer", strings: [1500, 1801, 1802] },
  { accessor: "Display.Damage", key: 0x5f, option: "damageDisplay", strings: [1500, 1805, 1806, 1807] },
  { accessor: "Display.Map", key: 0x60, option: "map", strings: [1500, 1808, 1809, 1830] },
  { accessor: "Display.MapScale", key: 0x61, option: "mapScale", strings: [1810, 1811, 1812, 1813, 1814] },
  { accessor: "Display.OppList", key: 0x62, option: "opponentList", strings: [1500, 1832, 1833] },
  { accessor: "Display.SplitTimes", key: 0x63, option: "splitTimes", strings: [1500, 1816, 1817, 1819] },
  { accessor: "Display.DispCols", key: 0x64, option: "displayColors", strings: [1821, 1822, 1823] },
  { accessor: "Display.Translucency", key: 0x66, option: "translucency", strings: [1501, 1500] },
  { accessor: "Display.RearView", key: 0x67, option: "rearViewMirror", strings: [1500, 1501] },
  { accessor: "Display.RecordTime", key: 0x68, option: "recordTime", strings: [1500, 1501] },
  { accessor: "Display.Gear", key: 0x69, option: "gear", strings: [1500, 1501] },
];

// The hud2 options of config `index` as shipped (before any edit).
export function configOptions(records, index) {
  const r = records[index] ?? records[0];
  const out = {};
  if (r) OPTION_NAMES.forEach((key, i) => { out[key] = r.hud2.getInt32(0x30 + 4 * i, true); });
  return out;
}

// In-race HUD keys (fe.txt: I_TOGGLEHUD F5, I_TOGGLEREARVIEW F6, I_TOGGLEMAP F7,
// I_HUDTRANSPARENCY F8, I_CYCLEHUD 1, I_CYCLEMAP 2, I_ZOOMINMAP numpad -, I_ZOOMOUTMAP numpad +;
// actions 0x18..0x1f = ids 0x207..0x20e [VA 0x4b4274, dispatcher 0x4152e0]). They edit the
// selected record in memory (not the files) and keep the HUD state `s`:
// { shown, map, mapScale, rearViewMirror, savedMap, alpha }. I_CYCLEHUD's predicate 0x427ac0
// always returns 0 in this build, so key 1 does nothing.
export const HUD_KEYS = {
  F5: "toggleHud", F6: "toggleMirror", F7: "toggleMap", F8: "translucency",
  Digit2: "cycleMap", NumpadSubtract: "zoomIn", NumpadAdd: "zoomOut",
};
export function hudKey(s, action) {
  const n = { ...s };
  switch (action) {
    case "toggleHud": // 0x4273b0
      if (n.shown) {
        n.shown = false;
        n.mapWasOn = n.map !== 0;
        if (n.mapWasOn) { n.savedMap = n.map; n.map = 0; }
        n.mirrorWasOn = n.rearViewMirror !== 0;
        if (n.mirrorWasOn) n.rearViewMirror = 0;
      } else {
        n.shown = true;
        if (n.mapWasOn) n.map = n.savedMap ?? 1;
        if (n.mirrorWasOn) n.rearViewMirror = 1;
      }
      break;
    case "toggleMirror": // 0x427550
      if (n.rearViewMirror !== 0) n.rearViewMirror = 0;
      else if (n.shown) n.rearViewMirror = 1;
      break;
    case "toggleMap": // 0x427360: 0x41e6e0 saves and clears, 0x41e690 restores (1 if nothing saved)
      if (n.map !== 0) { n.savedMap = n.map; n.map = 0; }
      else if (n.shown) { n.map = n.savedMap ?? 1; n.savedMap = null; }
      break;
    case "cycleMap": // 0x426740: radar -> static, rotating -> radar, else -> rotating
      n.map = n.map === 3 ? 2 : n.map === 1 ? 3 : 1;
      break;
    case "zoomIn": // 0x427600 -> 0x41e620
      n.mapScale = n.mapScale === 0 ? 2 : Math.min(4, n.mapScale + 1);
      break;
    case "zoomOut": // 0x427630 -> 0x41e5e0
      n.mapScale = Math.max(1, n.mapScale - 1);
      break;
    case "translucency": // 0x4275d0: 0x41e5c0 ? 0x41e540 : 0x41e590
      n.alpha = n.alpha === 0 ? BACKING_ALPHA : 0;
      break;
    default:
      return s;
  }
  return n;
}

// Preview of the options screen (`NFS5.HUD` control, FEHudDisplayControl draw 0x4cb970): the
// road picture hud.fsh shape 0 over 240x180, then one hud.fsh shape per shown element at its slot
// fraction of the selected config * (240, 180), sized shape * (640/W, 480/H). Order and shapes:
// damage (1 car icon: 3 at S3; 2 bar graph: 1 at S3; 3 percentages: 2 at S5), map (1, 2: 4;
// 3: 5 at S8), opponent list (6 at S4), mirror (7 at S10), split times (10 at S11), gear (11 at
// S0), tachometer (1: 12 at S12; 2: 14 at S6), speedometer (1: 12 at S1; 2: 13 at S7).
// `cfg` from selectConfig at W x H; `sizes(shape)` -> [w, h]. Items {shape, x, y, w, h} relative
// to the control.
export const PREVIEW_W = 240;
export const PREVIEW_H = 180;
export function hudPreviewDraws(cfg, W, H, sizes) {
  const o = cfg.options;
  const out = [{ shape: 0, x: 0, y: 0, w: PREVIEW_W, h: PREVIEW_H }];
  const sx = DESIGN_W / W;
  const sy = DESIGN_H / H;
  const put = (shape, slot) => {
    const [fx, fy] = cfg.fractions[slot];
    const [w, h] = sizes(shape);
    out.push({ shape, x: fx * PREVIEW_W, y: fy * PREVIEW_H, w: w * sx, h: h * sy });
  };
  if (o.damageDisplay === 1) put(3, 3);
  else if (o.damageDisplay === 2) put(1, 3);
  else if (o.damageDisplay === 3) put(2, 5);
  if (o.map === 1 || o.map === 2) put(4, 8);
  else if (o.map === 3) put(5, 8);
  if (o.opponentList !== 0) put(6, 4);
  if (o.rearViewMirror !== 0) put(7, 10);
  if (o.splitTimes !== 0) put(10, 11);
  if (o.gear !== 0) put(11, 0);
  if (o.tachometer === 1) put(12, 12);
  else if (o.tachometer === 2) put(14, 6);
  if (o.speedometer === 1) put(12, 1);
  else if (o.speedometer === 2) put(13, 7);
  return out;
}

// Gauge settings of a car: nfs5.car record (0x670 bytes) +0x300..+0x322, written by the front end
// as race-config keys 0x6a..0x74 [VA 0x4eb710; data nfs5.car].
export function carGauges(record, units = 0) {
  const dv = new DataView(record.buffer, record.byteOffset, record.byteLength);
  const s = off => dv.getInt16(off, true);
  const mph = units !== 0;
  return {
    speedo: {
      style: s(mph ? 0x30c : 0x300), total: s(mph ? 0x30e : 0x302),
      start: s(mph ? 0x312 : 0x306), perUnit: s(mph ? 0x314 : 0x308) * 0.001,
    },
    tach: { style: s(0x318), total: s(0x31a), start: s(0x31e), perUnit: s(0x320) * 0.001 },
    needleStyle: s(0x31c),
    needlePivot: s(0x322),
  };
}

// Needle sizes per NEEDLESTYLE (tables 0x5ccf18 / 0x5ccfe0, equal) = needle*.fsh shapes 0..4.
export const NEEDLE_SIZES = [[5, 74], [19, 60], [18, 65], [17, 71], [17, 64]];

// Pack names: speed table 0x5ccefc [night + 2 * units], needle 0x5ccf0c [night];
// tach 0x5ccfcc [night], tach needle 0x5ccfd4 [night] [VA 0x42e710, 0x42f3a0].
export function gaugePacks(night, units) {
  const n = night ? 1 : 0;
  return {
    speedFace: ["speedK", "speedKN", "speedM", "speedMN"][n + 2 * (units ? 1 : 0)],
    speedNeedle: ["needle2", "needle2N"][n],
    tachFace: ["tach", "tachN"][n],
    tachNeedle: ["needle", "needleN"][n],
    backing: "circletach",
  };
}

// Dial value in gauge units [VA 0x42e840, 0x42f660]: speedometer ftol(v * 3.6 or * 2.2374139),
// v = car+0xec4 in m/s; tachometer ftol(rpm * (R - 750) / R + 750) / 10, R = redline car+0xee0.
// The x87 runs at 53-bit precision (CRT start-up): f32 · f32 is exact in a double and `ftol`
// truncates it unrounded, so no f32 rounding before the truncation (recorded,
// replay-hud-gauges.py: 50 m/s · 3.5999999 = 179.999995 shows 179).
export function speedoUnits(speedEc4, units = 0) {
  return Math.trunc(f32(speedEc4) * (units ? 2.2374138832092285 : 3.5999999046325684));
}
// HTach ctor tail [VA 0x42f501..0x42f532]: this+0xa9d0 = f32((R - 750) / R), R the int car+0xee0.
export function tachFactor(redline) {
  const R = Math.trunc(redline);
  return R > 0 ? f32((R - 750) / R) : 0;
}
export function tachUnits(rpm, redline) {
  if (!(Math.trunc(redline) > 0)) return 0;
  return Math.trunc(Math.trunc(f32(rpm) * tachFactor(redline) + 750) / 10);
}

// Needle angle (radians, counter-clockwise, y up) for a dial value [VA 0x4212a0, 0x421650]:
// the table runs from the start angle at the last unit back to unit 0, `perUnit` degrees a unit,
// with 3.14 for pi; values at or above `total` show `total - 1`.
export function needleAngle(value, gauge) {
  const total = Math.max(1, gauge.total);
  const v = Math.max(0, Math.min(total - 1, value));
  const deg = gauge.start + (total - 1 - v) * gauge.perUnit;
  return deg * 3.14 * 0.005555555555555556;
}

// Needle quad corners around the dial centre, screen coordinates [VA 0x4212a0, 0x421650]:
// [tip-left, tip-right, back-right, back-left]; `pivot` pixels of the needle are behind the centre.
export function needleQuad(cx, cy, angle, size, pivot) {
  const [w, len] = size;
  const half = Math.trunc((w + 1) / 2);
  const px = Math.cos(angle - 1.57) * half;
  const py = Math.sin(angle - 1.57) * half;
  const dx = Math.cos(angle);
  const dy = Math.sin(angle);
  const pt = (x, y) => [cx + x, cy - y];
  return [
    pt((len - pivot) * dx - px, (len - pivot) * dy - py),
    pt(px + (len - pivot) * dx, py + (len - pivot) * dy),
    pt(px - pivot * dx, py - pivot * dy),
    pt(-pivot * dx - px, -pivot * dy - py),
  ];
}

// Gauge box -> centre [VA 0x421770]: position + size / 2 (integer halves).
export function dialCentre(box) {
  return [box.x + Math.trunc(box.w / 2), box.y + Math.trunc(box.h / 2)];
}

// "mm:ss.hh" race time.
export function formatHudTime(seconds) {
  if (!Number.isFinite(seconds) || seconds < 0) return "--:--.--";
  const hundredths = Math.floor(seconds * 100 + 1e-6);
  const m = Math.floor(hundredths / 6000);
  const s = Math.floor(hundredths / 100) % 60;
  const h = hundredths % 100;
  return `${String(Math.min(m, 99)).padStart(2, "0")}:${String(s).padStart(2, "0")}.${String(h).padStart(2, "0")}`;
}

// Accent colour of values by DISPLAYCOLORS (hud2 +0x50): orange, red, green [VA 0x4204f0].
export const ACCENT_COLORS = [0xccfab400, 0xccff0606, 0xcc1ee605];
export function accentColor(displayColors) {
  return ACCENT_COLORS[displayColors] ?? ACCENT_COLORS[0];
}

// car+0xd82 gear -> text: 0 "R" (hud string 18145), 1 "N" (18146), 2.. "1".. (table 0x5ccc10)
// [VA 0x421cb0].
export function gearText(raw, strings = {}) {
  if (raw <= 0) return strings[18145] ?? "R";
  if (raw === 1) return strings[18146] ?? "N";
  return String(raw - 1);
}

// Original HUD time: 64 Hz ticks -> "MM:SS.hh", hundredths = (t & 63) * 100 / 64, capped at
// "59:59.99"; `signed` prefixes "+" or "-" [VA 0x4356e0].
export function formatTicks(ticks, signed = false) {
  let t = Math.trunc(ticks);
  const sign = t < 0 ? "-" : "+";
  t = Math.abs(t);
  let text;
  const secs = Math.trunc(t / 64);
  if (secs >= 3600) text = "59:59.99";
  else {
    const hh = Math.trunc(((t & 63) * 100) / 64);
    text = `${String(Math.trunc(secs / 60)).padStart(2, "0")}:${String(secs % 60).padStart(2, "0")}.${String(hh).padStart(2, "0")}`;
  }
  return signed ? sign + text : text;
}
export const secondsToTicks = s => Math.trunc(s * 64);

// Panel box [VA 0x434110]: black quad, alphas 63 (top) -> 255 (bottom), times fade/100.
export const BOX_ALPHA_TOP = 63;
export const BOX_ALPHA_BOTTOM = 255;

// Damage ramp of HGraphicDamage [VA 0x421e00, 0x422470]: levels 1..10 (the table holds an 11th, equal
// to the 10th); alpha 0xaa. Level 0 (zone damage below 10) is the undamaged colour 0x55058228.
export const DAMAGE_RAMP = [0x058228, 0x02912d, 0x009b32, 0xffe100, 0xffcd05, 0xffb405, 0xffa005,
  0xdc6e05, 0xdc4605, 0xdc1905].map(c => (0xaa000000 | c) >>> 0);
export const DAMAGE_UNDAMAGED = 0x55058228;
// Level of a zone value (car+0x4e4.., 0..100): ftol(zone * 10.0 [0x5b241c] * 0.01 [0x5b2bc0]) [VA 0x422470].
export function damageLevel(zone) {
  if (!(zone > 0)) return 0;
  return Math.trunc(Math.fround(zone * 10) * 0.01);
}
export function damageColor(zone) {
  const level = damageLevel(zone);
  if (level <= 0) return DAMAGE_UNDAMAGED;
  return DAMAGE_RAMP[Math.min(level, DAMAGE_RAMP.length) - 1];
}
// car.fsh parts [VA 0x421e00 ctor, 0x420a90..0x420ae0 sizes, 0x420af0..0x420b40 offsets in the box + (5,4)
// [VA 0x420950], 0x4229d0 placement, 0x422470 colours]. `shape` is the index in car.fsh (C1..C6),
// `zones` the car+0x4e4 zones of the first copy and of the mirrored copy. The art holds one half of
// the car: parts 0, 2, 3 are drawn a second time flipped vertically below the axis y = 27 (the outline
// height [VA 0x420940]) at y' = y + (27 - y) * 2 - h - 1 [VA 0x422470, 0x423c70]; parts 1, 4, 5 are
// symmetric and drawn once.
export const DAMAGE_OUTLINE_SHAPE = 6;
export const DAMAGE_OUTLINE_H = 27;
export const DAMAGE_PARTS = [
  { shape: 3, size: [52, 13], at: [2, 2], zones: [2, 0], mirror: true },
  { shape: 0, size: [49, 44], at: [1, 5], zones: [1], mirror: false },
  { shape: 4, size: [41, 9], at: [54, 1], zones: [3, 7], mirror: true },
  { shape: 5, size: [24, 14], at: [95, 4], zones: [4, 6], mirror: true },
  { shape: 2, size: [9, 19], at: [110, 17], zones: [5], mirror: false },
  { shape: 1, size: [31, 35], at: [64, 9], zones: [8], mirror: false },
];
// Top of the mirrored copy of a part, relative to the silhouette origin.
export function mirrorY(part) {
  return part.at[1] + (DAMAGE_OUTLINE_H - part.at[1]) * 2 - part.size[1] - 1;
}
// Draw list of the silhouette: {shape, x, y, w, h, flip, zone} relative to the silhouette origin.
export function damageDraws() {
  const out = [{ shape: DAMAGE_OUTLINE_SHAPE, x: 0, y: 0, flip: false, zone: -1 },
    { shape: DAMAGE_OUTLINE_SHAPE, x: 0, y: DAMAGE_OUTLINE_H - 1, flip: true, zone: -1 }];
  for (const p of DAMAGE_PARTS) {
    out.push({ shape: p.shape, x: p.at[0], y: p.at[1], w: p.size[0], h: p.size[1], flip: false, zone: p.zones[0] });
    if (p.mirror) {
      out.push({ shape: p.shape, x: p.at[0], y: mirrorY(p), w: p.size[0], h: p.size[1], flip: true, zone: p.zones[1] });
    }
  }
  return out;
}

// --- Digital gauges (SPEEDOMETER / HUDTACHOMETER == 1) -----------------------------------------
// HSpeed (S1) [VA 0x42e570]: ftol(car+0xec4 * 3.6 | * 2.2374139), label "km/h" (11) / "mph" (20);
// zero-padded to 3 digits ("00" below 10, "0" below 100); "000" while car+0xec4 < 1.0 [0x5b24a8].
export function digitalSpeedText(speedEc4, units = 0) {
  if (!(speedEc4 >= 1)) return "000";
  const v = speedoUnits(speedEc4, units);
  return v < 10 ? `00${v}` : v < 100 ? `0${v}` : String(v);
}
// HTachDigital (S12) [VA 0x42f740]: ftol(car+0xdb4) rpm, label "rpm" (18144); padded to 4 digits,
// "0000" below 1.
export function digitalRpmText(rpm) {
  const v = Math.trunc(rpm);
  if (!(v >= 1)) return "0000";
  return v < 10 ? `000${v}` : v < 100 ? `00${v}` : v < 1000 ? `0${v}` : String(v);
}

// --- Text damage panel HTextDamage (S3, DAMAGEDISPLAY 2 bar graph / 3 percentages) -------------
// Rows [VA 0x42f900 ctor, 0x430870 layout, 0x430150 update]: labels 13 drivetrain, 14 steering,
// 15 body, 16 handling at y + trunc(h * i / 5), x + padX (unviers9b, list grey +0x1c); values
// right-aligned at x + w - padX (unviers9bi, accent), only in mode 3 ("N%", "0%" for 0); label
// "damage" (12) in UniversC12b grey +0x18 at the bottom like the combo labels. Values (ftol):
// drivetrain Car_DamageEngine 0x4938a0, steering (zone 0 + zone 2) / 2 0x493960, body sum of
// zones 0..8 * 0.083333336 0x493c90, handling Car_AlignmentNoise 0x493bc0. Without installed
// parts (Quick Race, A30) 0x4938a0 is zone 1 and 0x493bc0 is 0.
export const TEXT_DAMAGE_LABELS = [13, 14, 15, 16];
export function textDamageValues(zones) {
  const z = i => Math.fround(zones[i] ?? 0);
  let body = 0;
  for (let i = 0; i < 9; i++) body += z(i);
  return [z(1), (z(0) + z(2)) * 0.5, body * 0.083333336, 0].map(v => Math.trunc(v));
}
export function textDamagePercent(v) {
  return v === 0 ? "0%" : `${v}%`;
}
export function textDamageRowY(box, i) {
  return box.y + Math.trunc((box.h * i) / 5);
}
// Bar graph of row i (mode 2) [VA 0x430535..0x430851]: from x + padX, width w - 2 padX, top
// y_i + 4 [0x5b2404] + label text height, height hud1 +0x3c; filled trunc(v * width / 100) in
// hud1 +0x28, the rest in hud1 +0x2c (not drawn at 100).
export function textDamageBars(box, i, value, padX, textH, barH, fill, rest) {
  const x = box.x + padX;
  const w = box.w - 2 * padX;
  const y = textDamageRowY(box, i) + 4 + textH;
  const f = Math.trunc((value * w) / 100);
  const out = [{ x, y, w: f, h: barH, color: fill }];
  if (value !== 100) out.push({ x: x + f, y, w: w - f, h: barH, color: rest });
  return out;
}

// --- Wrong way HWrongWay (ctor 0x436ea0, update 0x436f70, layout 0x437360) ----------------------
// Not one of the configurable elements: shown while car+0xd30 != 0 (counted by 0x4a7660 while the
// car points against its track node), after step 0x1c0, outside race type 9 and replays
// [VA 0x425950]. Text 18159 in UniversC12b, colour 0xfffab400 (fixed, not DISPLAYCOLORS).
export const WRONG_WAY_COLOR = 0xfffab400;
// Blink [VA 0x436f70]: timer += step - lastStep (lastStep only advances while the element is
// updated); visible -> hidden after more than 0x40 steps, hidden -> visible after more than 0x20.
// The state starts visible with lastStep 0, so the first update after a long gap hides it first.
export function wrongWayBlink(prev, step) {
  const s = prev ?? { timer: 0, visible: true, last: 0 };
  let timer = s.timer + (step - s.last);
  let visible = s.visible;
  if ((visible && timer > 0x40) || (!visible && timer > 0x20)) {
    visible = !visible;
    timer = 0;
  }
  return { timer, visible, last: step };
}
// Layout [VA 0x437360]: centre (W/2, H/2) (integer halves); text at (ftol(cx - w/2), cy); frame
// left/right ftol(cx -/+ w/2 -/+ 5), top cy - 5, bottom ftol(cy + h + 5); each corner two 5 px
// lines [VA 0x436fcb..0x437345, line queue 0x419e90]. Lines as [x0, y0, x1, y1].
export function wrongWayLines(W, H, textW, textH) {
  const cx = Math.trunc(W / 2);
  const cy = Math.trunc(H / 2);
  const tx = cx - textW * 0.5;
  const L = Math.trunc(tx - 5);
  const R = Math.trunc(cx + textW * 0.5 + 5);
  const T = cy - 5;
  const B = Math.trunc(cy + textH + 5);
  return {
    textX: Math.trunc(tx), textY: cy,
    lines: [
      [L, T, L + 5, T], [L, T, L, T + 5],
      [L, B, L + 5, B], [L, B, L, B - 5],
      [R - 5, T, R, T], [R, T, R, T + 5],
      [R - 5, B, R, B], [R, B, R, B - 5],
    ],
  };
}

// --- Rear-view mirror HRearview (S10, ctor 0x42c510, update 0x42c740) -------------------------
// No tray box (ctor 0x433f90 with the no-box flag); the 3D view fills the slot; around it a bevel
// frame of 4 px ([0x5cce30]) drawn by 0x435a20 when the box did not move since the last frame and
// the no-alpha mode is off. Style 0 ([0x60cb34], no writer found): per side the outer edge and the
// inner edge colours (ARGB) of a Gouraud quad.
export const MIRROR_FRAME = { top: [0x05ffffff, 0x4c000000], right: [0x4c000000, 0x05ffffff],
  bottom: [0x4c000000, 0x05ffffff], left: [0x05ffffff, 0x4c000000] };
// Quads [VA 0x435a20]: pts = [outer0, outer1, inner1, inner0] for each side.
export function mirrorFrameQuads(x, y, w, h, b) {
  const oTL = [x - b, y - b], oTR = [x + w + b, y - b], oBR = [x + w + b, y + h + b], oBL = [x - b, y + h + b];
  const iTL = [x, y], iTR = [x + w, y], iBR = [x + w, y + h], iBL = [x, y + h];
  const q = (side, pts) => ({ side, pts, outer: MIRROR_FRAME[side][0], inner: MIRROR_FRAME[side][1] });
  return [
    q("top", [oTL, oTR, iTR, iTL]),
    q("right", [oTR, oBR, iBR, iTR]),
    q("bottom", [oBR, oBL, iBL, iBR]),
    q("left", [oBL, oTL, iTL, iBL]),
  ];
}

// Map zoom Z (half width of the map box in metres) [VA 0x41e9c0]: MAPSCALE 0..4 -> s = 0, 0, 33,
// 66, 100; Z = 185 + 4.15 s (radar mode MAP 3: 5 + 0.15 s); Z = 600 means the whole track
// (centre on the outline's bounding box, Z = max extent / 2 + 200) [VA 0x41ea40, 0x433440].
export function mapZoom(map, mapScale) {
  const s = [0, 0, 33, 66, 100][mapScale] ?? 0;
  if (map === 3) return { z: 5 + 0.15 * s, whole: false };
  const z = 185 + 4.15 * s;
  return { z, whole: Math.round(z) >= 600 };
}
// Marker size factor k: car footprint times 20, 60 for the whole track, 1 in radar mode [VA 0x433110].
export function markerScale(map, whole) {
  return map === 3 ? 1 : whole ? 60 : 20;
}

// Track map from `viewer.get_course_map()` [VA 0x432230 built in the engine from the node graph]:
// 4 header floats (bounding box centre x, z, extent, 1 for a circuit) then line segments
// x0, z0, x1, z1. Segments never join two sections (the original draws a segment only between
// consecutive outline points of one section [VA 0x433690]).
export function parseCourseMap(raw) {
  if (!raw || raw.length < 8) return { segs: [], bbox: null, closed: false };
  return {
    bbox: { cx: raw[0], cz: raw[1], ext: raw[2] },
    closed: raw[3] !== 0,
    segs: raw.slice(4),
  };
}

// Map view transform [VA 0x433440]: world (x, z) -> box pixels, centred on (cx, cz), scale
// box half width / Z, rotated so that `upYaw` (heading angle of the up direction, radians from
// +z towards +x) points up; `mirror` flips x.
export function mapTransform(box, cx, cz, zoom, upYaw, mirror = false) {
  const scale = box.w / 2 / zoom;
  const c = Math.cos(upYaw);
  const s = Math.sin(upYaw);
  const ox = box.x + box.w / 2;
  const oy = box.y + box.h / 2;
  return (x, z) => {
    const dx = x - cx;
    const dz = z - cz;
    // Rotate so the up heading maps to screen up (-y).
    let rx = dx * c - dz * s;
    const rz = dx * s + dz * c;
    if (mirror) rx = -rx;
    return [ox + rx * scale, oy - rz * scale];
  };
}

// --- Countdown H321Go (docs/re/hud.md "Countdown") -------------------------------------------
// HUD object vtable 0x5b27ec, ctor 0x417a60, draw 0x4184f0, digits 0x418670, wedge 0x418910.
// Visible from scheduler step 0x100 to 0x1c0 (64 steps/s, same steps as the phrases "3", "2", "1").
export const COUNTDOWN_FIRST_STEP = 0x100; // g_CountdownSteps 0x5b27d8..0x5b27e4
export const COUNTDOWN_STAGE_STEPS = [0x100, 0x140, 0x180];
export const COUNTDOWN_LAST_STEP = 0x1c0;
// Zoom records 0x81 + 5k (0x417a60): [scale, ARGB]; k = 0..4 have scale 0 (not drawn).
export const COUNTDOWN_ZOOM = [
  [0, 0x80ffffff], [0, 0x80ffffff], [0, 0x80ffffff], [0, 0x80ffffff], [0, 0x80ffffff],
  [8, 0x3fffffff], [7, 0x3fffffff], [6, 0x3fffffff], [5, 0x3fffffff],
  [4, 0x80ffffff], [3, 0x80ffffff], [2, 0x80ffffff],
  [1, 0xbfffffff], [1, 0xbfffffff], [1, 0x80ffffff], [1, 0x80ffffff],
];
export const COUNTDOWN_CIRCLE_FILL = 0x3ffab400; // 0x4184f0
export const COUNTDOWN_CIRCLE_RING = 0x80fab400;
export const COUNTDOWN_LINE = 0x80000000; // 0x419c50
export const COUNTDOWN_WEDGE = 0x33000000; // 0x418910 -> 0x466100
// Digit art per stage [pack, frame] from `FUN_00422df0(name, frame, ...)` in 0x417a60 (sizes DAT_005cc838..).
export const COUNTDOWN_ART = [
  { solid: ["321go", 1], outline: ["321go", 2] }, // 3sol, 3ol
  { solid: ["321go", 0], outline: ["321go", 3] }, // 2sol, 2ol
  { solid: ["321go2", 0], outline: ["321go2", 1] }, // 1sol, 1ol
];

// The HUD elements (dials, gear, time, list, damage, map) are drawn only once the scheduler step
// is past the start, `g_SchedStep > 0x1c0` [VA 0x425a29, 0x425bab]: nothing during the race intro
// and the countdown; H321Go is drawn on its own window. Phase 1 = the 7 s start (countdown).
export function hudElementsVisible(phase) {
  return phase !== 1;
}

// Stage (0..2), sweep tick k (0..15, 1/15 s) and the step within the second for scheduler step
// `step`; null outside the countdown (0x4184f0).
export function countdownState(step) {
  if (step < COUNTDOWN_FIRST_STEP || step > COUNTDOWN_LAST_STEP) return null;
  const stage = step >= COUNTDOWN_STAGE_STEPS[2] ? 2 : step >= COUNTDOWN_STAGE_STEPS[1] ? 1 : 0;
  const n = step % 64;
  const k = Math.trunc((n * 3 + 3) * 5 / 64);
  return { stage, n, k };
}

// The 16 border points of the wedge fan, clockwise from the top centre (0x418910), for a W x H view.
export function wedgePoints(W, H) {
  const xs = [W / 2, 3 * W / 4, W, W, W, W, W, 3 * W / 4, W / 2, W / 4, 0, 0, 0, 0, 0, W / 4];
  const ys = [0, 0, 0, H / 4, H / 2, 3 * H / 4, H, H, H, H, H, 3 * H / 4, H / 2, H / 4, 0, 0];
  return xs.map((x, i) => [x, ys[i]]);
}

// Draw list of the countdown at `step` on a W x H view (HUD units = pixels, art 1:1).
// `sizes(pack, frame)` -> [w, h] of the art. Items in draw order:
//  { kind: "sprite", art: [pack, frame], x, y, w, h, color } | { kind: "line", x0, y0, x1, y1, color }
//  | { kind: "wedge", tris: [[p0, p1, p2]...], color }
export function countdownDraws(step, W, H, sizes) {
  const s = countdownState(step);
  if (!s) return null;
  const { stage, k } = s;
  const cx = Math.trunc(W / 2);
  const cy = Math.trunc(H / 2);
  const out = [];
  // Scaled quad (0x418670): corners centre -/+ trunc(size * scale) / 2.
  const zoomed = (art, w, h, color, scale) => {
    const hw = Math.trunc(w * scale) >> 1;
    const hh = Math.trunc(h * scale) >> 1;
    out.push({ kind: "sprite", art, x: cx - hw, y: cy - hh, w: hw + hw, h: hh + hh, color });
  };
  // Unscaled art centred on the screen centre (outlines, rings) or in the 130 px box (solid digits, 0x417a60).
  const centred = (art, w, h, color) => {
    out.push({ kind: "sprite", art, x: cx - (w >> 1), y: cy - (h >> 1), w, h, color });
  };
  const boxed = (art, w, h, color) => {
    const bx = Math.trunc((W - 0x82) / 2) + ((0x82 - w) >> 1);
    const by = Math.trunc((H - 0x82) / 2) + ((0x82 - h) >> 1);
    out.push({ kind: "sprite", art, x: bx, y: by, w, h, color });
  };
  // Backing disc and ring, 128 px (0x417a60: circlefull / circleoutline).
  centred(["circlefull", 0], 128, 128, COUNTDOWN_CIRCLE_FILL);
  centred(["circleoutline", 0], 128, 128, COUNTDOWN_CIRCLE_RING);
  out.push({ kind: "line", x0: 0, y0: cy, x1: W, y1: cy, color: COUNTDOWN_LINE });
  out.push({ kind: "line", x0: cx, y0: 0, x1: cx, y1: H, color: COUNTDOWN_LINE });
  const art = COUNTDOWN_ART[stage];
  const [zoom, zcolor] = COUNTDOWN_ZOOM[k];
  if (zoom > 0) {
    const [w, h] = sizes(...art.solid);
    zoomed(art.solid, w, h, zcolor, zoom);
  }
  if (stage > 0 && k <= 4) { // the previous digit stays until the new one zooms in
    const prev = COUNTDOWN_ART[stage - 1];
    const [w, h] = sizes(...prev.solid);
    boxed(prev.solid, w, h, 0x80ffffff);
  }
  {
    const alpha = (k & 7) << 5; // (k << 0x1d) | 0xffffff: top 3 bits of the alpha byte
    const [w, h] = sizes(...art.outline);
    centred(art.outline, w, h, ((alpha << 24) | 0xffffff) >>> 0);
  }
  if (stage > 0) { // previous outline fades: alpha = (0x100 / k) << 24 (k = 0, 1 give 0)
    const a = (k === 0 ? 0x100 : Math.trunc(256 / k)) & 0xff;
    const prev = COUNTDOWN_ART[stage - 1];
    const [w, h] = sizes(...prev.outline);
    centred(prev.outline, w, h, ((a << 24) | 0xffffff) >>> 0);
  }
  const pts = wedgePoints(W, H);
  const tris = [];
  for (let i = 0; i <= k; i++) tris.push([[W / 2, H / 2], pts[i], pts[(i + 1) % 16]]);
  out.push({ kind: "wedge", tris, color: COUNTDOWN_WEDGE });
  return out;
}

// Colour of a car in the HUD (position list square, map marker): its paint colour
// `res+0x218` = COLORCONFIG[0], key 0x149 = the body colour of the car's colour entry as
// 0xAARRGGBB [VA 0x42b000, 0x433110 read it; 0x4eeda0 writes it, 0x4f4fa0 draws the entry].
// `colours[i]` is the 0xRRGGBB of car i (player first); cars without one keep a fixed palette
// (A213).
export const FALLBACK_CAR_COLOURS = [0xf2f2f2, 0xe8281e, 0xf5c518, 0x2f6fe0, 0x2fb84a, 0xee8a1a, 0x9b59d6, 0x18b8c8];
export function carHudColour(colours, index) {
  const c = colours?.[index];
  return c === undefined || c === null ? FALLBACK_CAR_COLOURS[index % FALLBACK_CAR_COLOURS.length] : c & 0xffffff;
}

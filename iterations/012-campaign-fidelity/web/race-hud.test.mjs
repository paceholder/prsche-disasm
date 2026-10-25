// Original race HUD logic (no DOM). Run: node --test web/*.test.mjs
// Uses NFS5_GAME_DIR or the repository's local/game installation.
import assert from "node:assert/strict";
import test from "node:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  anchorBox, parseHudFiles, selectConfig, carGauges, speedoUnits, tachUnits, needleAngle,
  needleQuad, dialCentre, SLOT, formatTicks, gearText, accentColor, mapZoom, markerScale,
  resampleOutline, mapTransform, damageColor, DAMAGE_UNDAMAGED, gaugePacks,
} from "./race-hud-core.js";

const gameDir = process.env.NFS5_GAME_DIR
  ?? fileURLToPath(new URL("../../../local/game/", import.meta.url));
const hasGame = existsSync(join(gameDir, "FEData/Data/nfs5-1.hud"));
const read = p => new Uint8Array(readFileSync(join(gameDir, p)));

test("quadrant anchoring of 640x480 positions (VA 0x41e480)", () => {
  assert.deepEqual(anchorBox(392, 352, 128, 128, 640, 480), [392, 352]);
  assert.deepEqual(anchorBox(392, 352, 128, 128, 800, 600), [552, 472]); // right, bottom
  assert.deepEqual(anchorBox(502, 1, 138, 15, 800, 600), [662, 1]); // right, top
  assert.deepEqual(anchorBox(0, 358, 120, 120, 800, 600), [0, 478]); // left, bottom
  assert.deepEqual(anchorBox(185, 0, 240, 84, 800, 600), [280, 0]); // across x = 320: centred
  assert.deepEqual(anchorBox(512, 307, 128, 128, 640, 480), [512, 307]);
  assert.deepEqual(anchorBox(600, 400, 128, 128, 640, 480), [512, 352]); // kept on screen
});

test("needle table: start angle at the last unit, clockwise with the value (VA 0x4212a0)", () => {
  const g = { total: 300, start: -40, perUnit: 0.91 };
  const deg = a => a / (3.14 / 180);
  assert.ok(Math.abs(deg(needleAngle(299, g)) - -40) < 1e-9);
  assert.ok(Math.abs(deg(needleAngle(0, g)) - (-40 + 299 * 0.91)) < 1e-9);
  assert.equal(needleAngle(500, g), needleAngle(299, g)); // clamped to total - 1
  // Pointing straight up around 156 km/h on the 300 km/h face (race-hud-chase.png).
  assert.ok(Math.abs(deg(needleAngle(156, g)) - 90.13) < 0.01);
  const q = needleQuad(100, 100, Math.PI / 2, [5, 74], 18);
  // Up: tip 56 px above the centre, back 18 px below, 3 px either side.
  assert.ok(Math.abs(q[0][1] - 44) < 0.01 && Math.abs(q[3][1] - 118) < 0.01);
  assert.ok(q[0][0] < 100 && q[1][0] > 100);
  assert.deepEqual(dialCentre({ x: 392, y: 352, w: 128, h: 128 }), [456, 416]);
});

test("dial values (VA 0x42e840, 0x42f660)", () => {
  assert.equal(speedoUnits(10), 36);
  assert.equal(speedoUnits(27.8), 100);
  assert.equal(speedoUnits(27.8, 1), 62);
  assert.equal(tachUnits(0, 6500), 75); // idle floor 750 rpm
  assert.equal(tachUnits(6500, 6500), 650);
  assert.equal(tachUnits(3000, 6000), 337);
  assert.deepEqual(gaugePacks(false, 0), { speedFace: "speedK", speedNeedle: "needle2", tachFace: "tach", tachNeedle: "needle", backing: "circletach" });
  assert.equal(gaugePacks(true, 1).speedFace, "speedMN");
});

test("texts: gear, 64 Hz time, accent colours", () => {
  assert.equal(gearText(0), "R");
  assert.equal(gearText(1), "N");
  assert.equal(gearText(5), "4");
  assert.equal(formatTicks(64 * 157 + 43), "02:37.67");
  assert.equal(formatTicks(64 * 3600), "59:59.99");
  assert.equal(formatTicks(-64 * 2, true), "-00:02.00");
  assert.equal(accentColor(0), 0xccfab400);
  assert.equal(accentColor(2), 0xcc1ee605);
  assert.equal(damageColor(0), DAMAGE_UNDAMAGED);
  assert.equal(damageColor(1), 0xaadc1905);
});

test("map zoom, markers, outline and view (VA 0x41e9c0, 0x433110, 0x432230, 0x433440)", () => {
  assert.deepEqual(mapZoom(1, 0), { z: 185, whole: false });
  assert.deepEqual(mapZoom(1, 4), { z: 600, whole: true });
  assert.ok(Math.abs(mapZoom(3, 4).z - 20) < 1e-9);
  assert.equal(markerScale(1, false), 20);
  assert.equal(markerScale(1, true), 60);
  assert.equal(markerScale(3, false), 1);
  // A straight line keeps only its ends; a square ring keeps its corners.
  const line = [];
  for (let i = 0; i <= 10; i++) line.push(i, 0);
  assert.equal(resampleOutline(line).length, 4);
  // Heading up = +z: a point ahead of the centre is above the box centre.
  const t = mapTransform({ x: 0, y: 0, w: 120, h: 120 }, 0, 0, 60, 0);
  assert.deepEqual(t(0, 30), [60, 30]);
  const r = mapTransform({ x: 0, y: 0, w: 120, h: 120 }, 0, 0, 60, Math.PI / 2);
  const [x, y] = r(30, 0); // heading +x is up
  assert.ok(Math.abs(x - 60) < 1e-9 && Math.abs(y - 30) < 1e-9);
});

test("shipped configs: Config 1 at 800x600 places the gauges of race-hud-chase.png", { skip: !hasGame }, () => {
  const records = parseHudFiles(read("FEData/Data/nfs5-1.hud"), read("FEData/Data/nfs5-2.hud"));
  assert.deepEqual(records.map(r => r.name), ["Config 1", "Config 2", "Config 3", "Config 4", "Config 2"]);
  const c640 = selectConfig(records, 0, 640, 480);
  assert.equal(c640.matched, true);
  assert.deepEqual(c640.slots[SLOT.SPEEDO], { x: 392, y: 352, w: 128, h: 128 });
  assert.equal(c640.options.speedometer, 2);
  assert.equal(c640.options.tachometer, 2);
  assert.equal(c640.options.map, 1);
  assert.equal(c640.translucency, 0x80);
  assert.deepEqual(c640.fonts, ["UniversC12b.ffn", "unviers9b.ffn", "unviers9bi.ffn"]);
  const c800 = selectConfig(records, 0, 800, 600);
  assert.equal(c800.matched, false);
  // Reference (800x600 scaled to 640x480): speedometer centre (493, 429), tach (589, 393).
  const [sx, sy] = dialCentre(c800.slots[SLOT.SPEEDO]);
  const [tx, ty] = dialCentre(c800.slots[SLOT.TACH]);
  assert.ok(Math.abs(sx * 0.8 - 493) <= 2 && Math.abs(sy * 0.8 - 429) <= 2, `${sx},${sy}`);
  assert.ok(Math.abs(tx * 0.8 - 589) <= 2 && Math.abs(ty * 0.8 - 393) <= 2, `${tx},${ty}`);
  // Config 2 has its own 1600x1200 record.
  assert.equal(selectConfig(records, 1, 1600, 1200).matched, true);
});

test("car gauge settings from nfs5.car (VA 0x4eb710)", { skip: !hasGame }, () => {
  const car = read("FEData/Data/nfs5.car");
  const rec = i => car.subarray(i * 0x670, (i + 1) * 0x670);
  const a356 = carGauges(rec(8));
  assert.deepEqual(a356.speedo, { style: 0, total: 200, start: -45, perUnit: 1.35 });
  assert.deepEqual(a356.tach, { style: 1, total: 600, start: -48, perUnit: 0.5 });
  assert.equal(a356.needleStyle, 0);
  assert.equal(a356.needlePivot, 18);
  const t930 = carGauges(rec(43));
  assert.equal(t930.speedo.style, 4);
  assert.equal(t930.speedo.total, 300);
  assert.equal(carGauges(rec(43), 1).speedo.total, 200);
});

// node --test web/race-summary.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import { summaryModel, tableRect, windowRect, timeText, RaceSummary, ROW_H } from "./race-summary.js";

const results = {
  ended: true, laps: 1, linear: true, cars: [
    { index: 0, state: 2, place: 2, time: 17312, best: 17312 },
    { index: 1, state: 2, place: 1, time: 15000, best: 15000 },
    { index: 2, state: 2, place: 3, time: 0, best: 0 },
  ],
};

test("rows in place order with the point-to-point columns (0x42bcd0, 0x42c210)", () => {
  const m = summaryModel(results, { names: ["M4GN2M77", "Dylan", "Parisso"], car: "356 Speedster 1600" });
  assert.deepEqual(m.widths, [30, 90, 140, 80]);
  assert.deepEqual(m.heads, ["", "NAME", "CAR", "TIME"]);
  assert.equal(m.rows[0].cells[1], "Dylan");
  assert.equal(m.rows[1].cells[0], "2nd");
  assert.equal(m.rows[1].player, true);
  assert.equal(m.rows[1].cells[3], "04:30.50"); // 17312 / 64 s
  assert.equal(m.rows[0].cells[2], "356 Speedster 1"); // 15 characters
  assert.equal(m.rows[2].cells[3], "---------------");
  assert.equal(m.title, "Race Summary");
});

test("circuit adds the best lap; strings from hud.loc", () => {
  const m = summaryModel({ ...results, linear: false }, {}, { 18113: "ЛУЧШИЙ КРУГ", 18116: "1ый" });
  assert.deepEqual(m.widths, [30, 90, 140, 100, 80]);
  assert.equal(m.heads[3], "ЛУЧШИЙ КРУГ");
  assert.equal(m.rows[0].cells[0], "1ый");
  assert.equal(m.rows[0].cells.length, 5);
});

test("table centred in the 500x325 window (0x42e8b0)", () => {
  const win = windowRect(640, 480);
  assert.deepEqual(win, { x: 70, y: 77, w: 500, h: 325 });
  const t = tableRect(win, [30, 90, 140, 80]);
  assert.equal(t.x, 70 + 80);
  assert.equal(t.h, 9 * ROW_H);
  assert.equal(t.y, 77 + ((325 - 180) >> 1));
  assert.equal(timeText(64 * 61 + 32), "01:01.50");
});

test("Space closes, Esc exits", () => {
  const s = new RaceSummary();
  assert.equal(s.key("Space"), null);
  s.open(summaryModel(results));
  assert.equal(s.key("KeyW"), null);
  assert.equal(s.key("Space"), "close");
  assert.equal(s.isOpen, false);
  s.open(summaryModel(results));
  assert.equal(s.key("Escape"), "exit");
});

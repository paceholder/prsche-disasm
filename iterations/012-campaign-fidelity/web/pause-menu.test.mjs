import test from "node:test";
import assert from "node:assert/strict";
import { PauseMenu, CODE, buttonYs, buttonX, windowRect } from "./pause-menu.js";

test("button rows for 640x480 follow the original formulas", () => {
  assert.deepEqual(buttonYs(480, 3), [194, 224, 254]);
  assert.deepEqual(buttonYs(480, 4), [179, 209, 239, 269]);
  assert.equal(buttonX(640), 249);
  assert.deepEqual(windowRect(640, 480), { x: 70, y: 77, w: 500, h: 325 });
});

test("pause menu rectangles", () => {
  const m = new PauseMenu(); m.open();
  assert.deepEqual(m.buttons().map(b => [b.x, b.y, b.w, b.h]),
    [[249, 194, 142, 19], [249, 224, 142, 19], [249, 254, 142, 19]]);
  assert.deepEqual(m.buttons().map(b => b.label), ["continue", "race again", "quit…"]);
  assert.equal(m.title(), "Pause Menu");
});

test("clicks return the original codes", () => {
  const m = new PauseMenu(); m.open();
  assert.equal(m.click(300, 200), CODE.CONTINUE);
  assert.equal(m.click(300, 230), CODE.RACE_AGAIN);
  assert.equal(m.click(10, 10), null);
});

test("quit opens the exit menu and cancel returns", () => {
  const m = new PauseMenu(); m.open();
  assert.equal(m.click(300, 260), CODE.QUIT);
  assert.equal(m.kind, "exit");
  assert.equal(m.title(), "exit");
  assert.deepEqual(m.buttons().map(b => b.label), ["exit to windows", "exit to game menu", "cancel"]);
  assert.deepEqual(m.buttons().map(b => b.y), [209, 239, 269]);
  assert.equal(m.click(300, 275), CODE.CANCEL);
  assert.equal(m.kind, "pause");
});

test("exit menu with view replay has the four original rows", () => {
  const m = new PauseMenu({ viewReplay: true }); m.open(); m.click(300, 260);
  assert.deepEqual(m.buttons().map(b => [b.code, b.y]), [[0x15, 179], [4, 209], [5, 239], [0x18, 269]]);
});

test("keyboard: arrows, enter, escape", () => {
  const m = new PauseMenu(); m.open();
  assert.equal(m.key("Enter"), null);
  m.key("ArrowDown"); m.key("ArrowDown");
  assert.equal(m.hot, 1);
  assert.equal(m.key("Enter"), CODE.RACE_AGAIN);
  assert.equal(m.key("Escape"), CODE.CONTINUE);
  m.click(300, 260);
  assert.equal(m.key("Escape"), CODE.CANCEL);
  assert.equal(m.kind, "pause");
});

test("strings come from hud.loc ids", () => {
  const m = new PauseMenu({ strings: { "18128": "Pausemenü" } }); m.open();
  assert.equal(m.title(), "Pausemenü");
});

test("layout follows the view size", () => {
  const m = new PauseMenu(); m.open(); m.resize(1280, 720);
  assert.equal(m.buttons()[0].x, 640 - 71);
  assert.equal(m.buttons()[0].y, Math.trunc(360 - 45.6));
});

// Front-end layout logic (no DOM). Run: node --test web/*.test.mjs
import assert from "node:assert/strict";
import test from "node:test";
import {
  backdropLayoutName, parseLayoutJson, attrInt, attrPair, parseColor, fitViewport,
  toLayoutPoint, transitionState, nextStack,
} from "./fe-core.js";

test("screen ids map to backdrop layouts (VA 0x4e1470)", () => {
  assert.equal(backdropLayoutName("ID_MOVIEOVAL"), "screenmovieoval");
  assert.equal(backdropLayoutName("ID_SCREEN"), "screenempty");
  assert.equal(backdropLayoutName("ID_OPTIONOVAL"), "screenoptionoval");
  assert.equal(backdropLayoutName("ID_SOMETHING"), "screenveryempty");
  assert.equal(backdropLayoutName(""), null);
});

test("layout JSON keeps order and control types", () => {
  const json = JSON.stringify([
    { index: 0, attrs: { id: "REAL.BLACK_640x480", room: "ID_GARAGE" } },
    { index: 1, attrs: { id: "NFS5.PICTURE", pos: "448,274", flip: "3" } },
    { index: 18, attrs: { id: "NFS5.Rectangle", color: "0xff000000", size: "640,24" } },
  ]);
  const sections = parseLayoutJson(json);
  assert.deepEqual(sections.map(s => s.type), ["BLACK_640X480", "PICTURE", "RECTANGLE"]);
  assert.deepEqual(attrPair(sections[1].attrs, "pos"), [448, 274]);
  assert.equal(attrInt(sections[1].attrs, "flip"), 3);
  assert.equal(attrInt({ flip: "" }, "flip", 0), 0);
  assert.equal(attrInt({ v: "0x10" }, "v"), 16);
  assert.deepEqual(parseColor("0xFFBC0000"), { a: 255, r: 188, g: 0, b: 0 });
  assert.deepEqual(parseColor("0x30FF0000"), { a: 48, r: 255, g: 0, b: 0 });
});

test("4:3 frame is centred with bars on wide screens", () => {
  const view = fitViewport(1920, 1080);
  assert.equal(view.scale, 2.25);
  assert.equal(view.x, 240);
  assert.equal(view.y, 0);
  assert.deepEqual(toLayoutPoint(view, 240 + 320 * 2.25, 240 * 2.25), [320, 240]);
});

test("transitions take 0.5 s (VA 0x4e09e0)", () => {
  assert.deepEqual(transitionState("FADE", 0), { alpha: 0, dx: 0, dy: 0 });
  assert.equal(transitionState("FADE", 250).alpha, 0.5);
  assert.equal(transitionState("LEFT", 250).dx, -320);
  assert.equal(transitionState("RIGHT", 0).dx, 640);
  assert.deepEqual(transitionState("RIGHT", 600), { alpha: 1, dx: 0, dy: 0 });
  assert.deepEqual(transitionState("SAME", 0), { alpha: 1, dx: 0, dy: 0 });
});

test("goto offsets push, replace and pop (VA 0x4d2100)", () => {
  let stack = nextStack([], "gamesetup", 1);
  stack = nextStack(stack, "singleplayer", 1);
  assert.deepEqual(stack, ["gamesetup", "singleplayer"]);
  // singleplayer -> quickrace uses offset -1: quick race replaces singleplayer.
  assert.deepEqual(nextStack(stack, "quickrace", -1), ["gamesetup", "quickrace"]);
  assert.deepEqual(nextStack(stack, "credits", 0), ["gamesetup", "credits"]);
});

test("loading screen tiles cover 640x480 with 45 shapes (VA 0x4a4a70)", async () => {
  const { loadingTileLayout } = await import("./frontend.js");
  const tiles = loadingTileLayout();
  assert.equal(tiles.length, 45);
  assert.deepEqual(tiles[19], [19, 608, 0]);
  assert.deepEqual(tiles[20], [20, 0, 32]);
  assert.deepEqual(tiles[30], [30, 0, 96]);
  assert.deepEqual(tiles[44], [44, 512, 352]);
});

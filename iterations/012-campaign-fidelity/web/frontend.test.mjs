// Front-end layout logic (no DOM). Run: node --test web/*.test.mjs
import assert from "node:assert/strict";
import test from "node:test";
import {
  backdropLayoutName, parseLayoutJson, attrInt, attrPair, parseColor, fitViewport,
  toLayoutPoint, transitionState, nextLevels, backLevels,
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
  assert.deepEqual(attrPair({ size: "150," }, "size", [50, 50]), [150, 0]);
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

test("goto offsets open pop-ups and keep history (VA 0x4d2100)", () => {
  let levels = nextLevels([], "gamesetup", 1);
  levels = nextLevels(levels, "singleplayer", 1);
  assert.deepEqual(levels, [["gamesetup"], ["singleplayer"]]);
  // singleplayer -> quickrace (offset -1) closes the pop-up and goes on in the main level.
  levels = nextLevels(levels, "quickrace", -1);
  assert.deepEqual(levels, [["gamesetup", "quickrace"]]);
  levels = nextLevels(levels, "location", 0);
  assert.deepEqual(levels, [["gamesetup", "quickrace", "location"]]);
  assert.deepEqual(backLevels(levels), [["gamesetup", "quickrace"]]);
  assert.deepEqual(backLevels([["gamesetup"], ["settings"]]), [["gamesetup"]]);
  assert.equal(backLevels([["gamesetup"]]), null);
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

test("quick race settings follow the original lists (VA 0x4f9790, 0x4fc0b0)", async () => {
  const { QuickRaceSetup, quickRaceTracks, carColourCss } = await import("./fe-quickrace.js");
  const strings = { 300: "Monte Carlo Circuit 1", 340: "Monte Carlo 1", 301: "Alps Run", 341: "Alps",
    410: "forward", 411: "backward", 412: "forward/mirrored", 413: "backward/mirrored",
    400: "heavy", 401: "medium", 402: "light", 403: "none", 420: "1", 421: "2", 422: "3",
    640: "same car", 641: "random car", 642: "same era", 643: "same class", 644: "same era/class",
    645: "era/class", 451: "advanced" };
  const records = [
    { internal_name: "monaco1", name_string: 300, circuit: true, unlock: 0, difficulty: 1 },
    { internal_name: "skidpad", name_string: 316, circuit: true, unlock: 0, difficulty: 0 },
    { internal_name: "alps", name_string: 301, circuit: false, unlock: 0, difficulty: 1 },
    { internal_name: "castle", name_string: 302, circuit: false, unlock: 3, difficulty: 2 },
  ];
  const tracks = quickRaceTracks(records, ["monaco1", "skidpad", "alps", "castle"]);
  assert.deepEqual(tracks.map(t => t.internal_name), ["monaco1", "alps"]);
  const cars = [{ display_name: "'59 356 A", short_name: "356 A", colours: [[0x592208ff, 0, 0, 0]] },
    { display_name: "'87 959", short_name: "959", colours: [] }];
  const q = new QuickRaceSetup({ strings, cars, tracks });
  assert.equal(q.accessor("Track1.Track(SHORT)").text(), "Monte Carlo 1");
  assert.equal(q.accessor("Track1.Track").text(), "Monte Carlo Circuit 1");
  assert.equal(q.accessor("Track1.Track(DIFFICULTY)").text(), "advanced");
  assert.equal(q.accessor("Track1.Traffic").text(), "none");
  assert.equal(q.accessor("Track1.Laps").text(), "2");
  assert.equal(q.accessor("OpponentCar.Car(SHORT)").text(), "same car");
  assert.equal(q.accessor("Game.NumOpponents").text(), "4");
  // (SHORT) of the merged list is the record's display name (rec +0x02), not +0x34.
  assert.equal(q.accessor("zMergedList.Car(SHORT)").text(), "'59 356 A");
  assert.equal(q.accessor("GenericCar.Colour").color(), "rgb(8,34,89)");
  assert.equal(carColourCss(0x0000ffff), "rgb(255,0,0)");
  assert.equal(q.accessor("No.Such"), null);
  // Car setup screen: automatic, aids off, roof up, damage on at creation.
  const setup = { 560: "manual", 561: "automatic", 1500: "off", 1501: "on", 1509: "up", 1510: "down" };
  const s2 = new QuickRaceSetup({ strings: setup, cars, tracks });
  assert.deepEqual(["Car.Transmission", "Car.ABS", "Car.Traction", "Car.Roof", "Game.Damage"]
    .map(n => s2.accessor(n).text()), ["automatic", "off", "off", "up", "on"]);
  assert.deepEqual([s2.raceParams().manualBox, s2.raceParams().damage], [false, true]);
  s2.accessor("Car.Transmission").step(1);   // wraps: automatic -> manual
  s2.accessor("Game.Damage").select(0);
  s2.accessor("Car.Roof").select(1);
  assert.deepEqual([s2.raceParams().manualBox, s2.raceParams().damage], [true, false]);
  s2.reset();                                // SETTYPEQUICKRACE keeps the setup choices
  assert.equal(s2.accessor("Car.Transmission").text(), "manual");
  let p = q.raceParams();
  assert.deepEqual([p.track.internal_name, p.laps, p.reverse, p.mirror, p.opponents], ["monaco1", 2, false, false, 4]);
  q.accessor("Track1.Direction(SHORT)").select(3);
  q.accessor("Track1.Laps").select(7);
  q.stepTrack(1);
  p = q.raceParams();
  // Alps is not a circuit: one lap whatever the lap setting.
  assert.deepEqual([p.track.internal_name, p.laps, p.reverse, p.mirror], ["alps", 1, true, true]);
  q.stepTrack(1);
  assert.equal(q.currentTrack().internal_name, "monaco1");
  q.accessor("zMergedList.Car").step(-1);
  assert.equal(q.currentCar().short_name, "959");
  q.reset();
  assert.deepEqual([q.car, q.track, q.direction, q.lapsIndex, q.traffic], [0, 0, 0, 1, 0]);
});

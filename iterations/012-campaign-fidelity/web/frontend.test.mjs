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

test("location and quick race choices reach the race start (VA 0x4fc0b0, 0x4eb470, 0x4ecf00)", async () => {
  const { QuickRaceSetup, raceStart, raceStartFromUrl, playerPaint, opponentPaints } =
    await import("./fe-quickrace.js");
  const strings = { 410: "forward", 411: "backward", 412: "forward/mirrored", 413: "backward/mirrored",
    450: "beginner", 451: "advanced", 452: "expert" };
  const tracks = [{ internal_name: "ALPS", name_string: 301, circuit: false, unlock: 0, difficulty: 1 },
    { internal_name: "monaco1", name_string: 300, circuit: true, unlock: 0, difficulty: 1 }];
  const cars = [{ display_name: "356", short_name: "356", colours: [[0x000000ff], [0x0000ffff]] }];
  const q = new QuickRaceSetup({ strings, cars, tracks });
  // The location screen: TRACK1INC steps the track, the combo lists set direction and traffic.
  q.stepTrack(1);
  q.accessor("Track1.Direction").select(3);
  q.accessor("Track1.Traffic").select(2);
  q.accessor("Track1.Laps").select(4);
  q.accessor("Game.NumOpponents").select(7);
  q.accessor("Game.Difficutly").select(2);
  q.accessor("OpponentCar.Car").select(2);
  const s = raceStart(q.raceParams());
  assert.deepEqual([s.track, s.laps, s.reverse, s.mirror, s.traffic, s.opponents, s.opponentMode, s.difficulty],
    ["monaco1", 5, true, true, 2, 7, 2, 2]);
  // Directions: 0 forward, 1 backward, 2 forward/mirrored, 3 backward/mirrored.
  for (const [dir, reverse, mirror] of [[0, false, false], [1, true, false], [2, false, true], [3, true, true]]) {
    q.accessor("Track1.Direction").select(dir);
    const r = raceStart(q.raceParams());
    assert.deepEqual([r.reverse, r.mirror], [reverse, mirror], `direction ${dir}`);
  }
  // Point-to-point tracks race one lap whatever the lap selector says.
  q.stepTrack(-1);
  assert.equal(raceStart(q.raceParams()).laps, 1);
  // Debug URL.
  const u = raceStartFromUrl(new URLSearchParams("race=alps&dir=1&opponents=9&difficulty=2&traffic=1&laps=4"));
  assert.deepEqual([u.reverse, u.mirror, u.opponents, u.difficulty, u.traffic, u.laps], [true, false, 7, 2, 1, 4]);
  const d = raceStartFromUrl(new URLSearchParams(""));
  assert.deepEqual([d.reverse, d.mirror, d.opponents, d.difficulty, d.laps], [false, false, 3, 1, 3]);
  // Paints: the chosen list entry (packed A,R,G,B bytes) and a random entry per opponent.
  assert.deepEqual(playerPaint(cars[0], 1), [1, 0, 0, 1]);
  assert.equal(playerPaint(cars[0], 5), null);
  const picks = [1, 0, 1];
  let i = 0;
  assert.deepEqual(opponentPaints(cars[0], 3, () => picks[i++]), [[1, 0, 0, 1], [0, 0, 0, 1], [1, 0, 0, 1]]);
  assert.deepEqual(opponentPaints({ colours: [] }, 3, () => 0), []);
});

// ---- track pictures and the Evolution event info (A220, A221) ----
import { wallMap, smallMap, isBackward, isMirrored } from "./fe-trackmap.js";
import { EventInfoSetup, money } from "./fe-tournament.js";
import { wrapText } from "./frontend.js";

const shape = (name, cx = 0, cy = 0) => ({ name, width: 16, height: 16, cx, cy, canvas: {} });

test("wall tile: the F flag going backward, the S flag going forward, mirrored flips", () => {
  const shapes = [shape("map"), shape("p1"), shape("p2"), shape("p3"), shape("p4"), shape("s", 213, 102), shape("f", 103, 3)];
  const fwd = wallMap(shapes, 0);
  assert.deepEqual([fwd.flag.shape.name, fwd.flag.x, fwd.flag.y, fwd.flipX], ["s", 213, 102, false]);
  const back = wallMap(shapes, 1);
  assert.deepEqual([back.flag.shape.name, back.flag.x, back.flag.y], ["f", 103, 3]);
  assert.equal(wallMap(shapes, 2).flipX, true);
  assert.equal(isBackward(3) && isMirrored(3) && !isBackward(2), true);
  assert.equal(wallMap(null, 0), null);
  assert.equal(wallMap([shape("a"), shape("b")], 0), null);
});

test("small map sits at its header centre, with the flag at its own", () => {
  const shapes = [shape("coas", 184, 288), shape("s", 306, 355), shape("f", 226, 281)];
  const m = smallMap(shapes, 0);
  assert.deepEqual([m.x, m.y, m.flag.x, m.flag.y], [184, 288, 306, 355]);
  // 911 Cup, first race reversed: the flag at F, the top end of the route (reference frame).
  assert.deepEqual([smallMap(shapes, 1).flag.x, smallMap(shapes, 1).flag.y], [226, 281]);
});

test("event info accessors read the record like Tourn_DataAccessor", () => {
  const strings = { 1: "911 Cup", 2: "911", 3: "A challenge", 4: "Any 911", 450: "beginner", 301: "Cote d'Azur", 302: "Pyrenees", 321: "COTE D'AZUR", 322: "PYRENEES" };
  const events = [
    { id: 23, era: 0x209, record_type: 0, short_string: 2, title_string: 1, desc_string: 3, restriction_string: 4,
      entry_fee: 1000, prize_money: 12000, times_completed: 0, races: [{ track_id: 8, laps: 3, reverse: false }, { track_id: 4, laps: 3, reverse: true }] },
    { id: 31, era: 0x209, record_type: 2, short_string: 9, races: [] },
    { id: 2, era: 0x20a, record_type: 0, short_string: 9, races: [] },
  ];
  const tracks = [{ id: 8, name_string: 301, difficulty: 0, small_pack: "coastalSpSm" }, { id: 4, name_string: 302, difficulty: 1 }];
  const setup = new EventInfoSetup({ events, tracks, strings });
  assert.equal(setup.events.length, 1); // era list without the bonus race
  const text = sel => setup.accessor(`Tournament.Tournament${sel ? `(${sel})` : ""}`).text();
  assert.deepEqual(["ENTRYFEE", "PRIZEMONEY", "TIMESCOMPLETED", "CARRESTRICTION", "DIFFICULTY", "DESCRIPTION", "SHORT", ""].map(text),
    ["1,000", "12,000", "0", "Any 911", "beginner", "A challenge", "911", "911 Cup"]);
  assert.deepEqual(setup.locationNames(), ["COTE D'AZUR", "PYRENEES"]);
  assert.equal(setup.direction(), 0);
  assert.equal(setup.accessor("Track1.Track"), null);
  assert.equal(money(1234567), "1,234,567");
  assert.equal(money(0), "0");
});

test("description text wraps at spaces within the control width", () => {
  const font = { glyphs: new Map([[32, { advance: 2 }], ...[..."abcdef"].map(ch => [ch.charCodeAt(0), { advance: 5 }])]) };
  assert.deepEqual(wrapText(font, "abc def abc", 32), ["abc def", "abc"]);
  assert.deepEqual(wrapText(font, "", 22), []);
  assert.deepEqual(wrapText(font, "abcdef", 3), ["abcdef"]);
});

import { wallCycle, tileCount, formatRecordTime, WALL_IDLE_TICKS } from "./fe-trackmap.js";
test("wall cycle: idle 512 ticks, turn 64 ticks, next tile, wraps over the 5 tiles", () => {
  assert.deepEqual(wallCycle(0, 5), { from: 0, to: 1, turn: 0 });
  assert.deepEqual(wallCycle(WALL_IDLE_TICKS - 1, 5), { from: 0, to: 1, turn: 0 });
  assert.deepEqual(wallCycle(WALL_IDLE_TICKS + 32, 5), { from: 0, to: 1, turn: 0.5 });
  assert.deepEqual(wallCycle(576, 5), { from: 1, to: 2, turn: 0 });
  assert.equal(wallCycle(576 * 5, 5).from, 0);
  assert.equal(tileCount(new Array(7)), 5);
});
test("record time is 1/64 s as mm:ss.cc", () => {
  assert.equal(formatRecordTime(164 * 64), "02:44.00");
  assert.equal(formatRecordTime(14848), "03:52.00");
  assert.equal(formatRecordTime(4130), "01:04.53");
  assert.equal(formatRecordTime(0), "");
});

import { splineCycle } from "./fe-trackmap.js";
test("spline control: map index t>>8, fade in 32 ticks, fade out 32 ticks", () => {
  assert.deepEqual(splineCycle(0, 3), { index: 0, alpha: 0 });
  assert.deepEqual(splineCycle(16, 3), { index: 0, alpha: 128 });
  assert.deepEqual(splineCycle(100, 3), { index: 0, alpha: 255 });
  assert.deepEqual(splineCycle(255, 3), { index: 0, alpha: 7 });
  assert.equal(splineCycle(256, 3).index, 1);
  assert.equal(splineCycle(256 * 3, 3).index, 0);
});

test("car compare reads the five ratings of both cars (VA 0x4ca790, rec +0x324)", async () => {
  const { QuickRaceSetup } = await import("./fe-quickrace.js");
  const cars = [{ display_name: "'59 A", short_name: "A", year: 1959, group: 501, ratings: [30, 40, 56, 30, 41], colours: [] },
    { display_name: "'72 B", short_name: "B", year: 1972, group: 503, ratings: [79, 65, 67, 51, 69], colours: [] }];
  const q = new QuickRaceSetup({ strings: {}, cars, tracks: [] });
  const v = name => q.accessor(`CarCompare.Data(${name})`).value();
  assert.equal(v("0CarCompareCar1"), 30);
  assert.equal(v("4CarCompareCar1"), 41);
  q.accessor("GenericCar2.Car(SHORT)").select(1);
  assert.equal(q.accessor("GenericCar2.Car(SHORT)").text(), "B", "header: the short name");
  assert.deepEqual(q.accessor("GenericCar2.Car").rows(), [{ year: "'59", text: "A", index: 0 },
    { sep: true, text: "" }, { year: "'72", text: "B", index: 1 }], "year | name, separator at a class change");
  assert.equal(q.accessor("CarCompare.Data(2CarCompareCar2)").text(), "67");
  assert.equal(v("2CarCompareCar1"), 56, "car 1 follows the car list, car 2 GenericCar2");
});

test("click zones of the garage car follow the room yaw (VA 0x4c0d10)", async () => {
  const { carClickZone } = await import("./fe-car-click.js");
  const rect = { left: 110, top: 230, right: 530, bottom: 410 };   // QuickRace.lay 3DCAR 420x180
  // Top 5/16 of the control is the cabrio top, whatever the yaw.
  assert.equal(carClickZone(300, 250, rect, 0.5, true), 0);
  assert.equal(carClickZone(300, 250, rect, 0.5, false), 3, "no top channel: hood seen from the front");
  assert.equal(carClickZone(300, 350, rect, 0.0, true), 4, "seen from behind: trunk");
  assert.equal(carClickZone(300, 350, rect, 0.5, true), 3, "seen from the front: hood");
  // Side views: sixths of the width are hood and trunk, the middle is a door.
  assert.equal(carClickZone(120, 350, rect, 0.75, true), 3);
  assert.equal(carClickZone(300, 350, rect, 0.75, true), 1);
  assert.equal(carClickZone(520, 350, rect, 0.75, true), 4);
  assert.equal(carClickZone(120, 350, rect, 0.25, true), 4);
  assert.equal(carClickZone(300, 350, rect, 0.25, true), 2);
  assert.equal(carClickZone(520, 350, rect, 0.25, true), 3);
});

test("part tweens follow the 128 Hz level formulas of 0x4c0520", async () => {
  const { carPartMs, ZONE_CHANNEL } = await import("./fe-car-click.js");
  assert.ok(Math.abs(carPartMs(1, true) - 2988) < 1, "top: level 2*ticks/3 reaches 255 at 382 ticks");
  assert.ok(Math.abs(carPartMs(6, true) - 601.6) < 1, "door opens in 255*0.302 ticks");
  assert.ok(Math.abs(carPartMs(7, false) - 775) < 1, "door closes in 255*0.389 ticks");
  assert.equal(carPartMs(8, true), 1000);
  assert.deepEqual(ZONE_CHANNEL, { 0: 1, 1: 6, 2: 7, 3: 8, 4: 9 });
});

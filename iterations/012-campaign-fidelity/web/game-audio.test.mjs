// Pure helpers of the original music and race speech (web/game-audio.js).
import assert from "node:assert/strict";
import test from "node:test";
import {
  announcerStep, countdownDigit, countdownStep, parseFeAudio, parseVivToc, RaceEvents, ScreenMusic,
  screenMusicPattern, START_SECS, VolumeRamp,
} from "./game-audio.js";

test("viv table of contents", () => {
  const names = ["game13-Activator-Morphadron", "menu01-a"];
  const toc = [];
  names.forEach((n, i) => {
    toc.push(0, 0, 0, 100 + i, 0, 0, 0, 10 + i, ...[...n].map(c => c.charCodeAt(0)), 0);
  });
  const head = [..."BIGF"].map(c => c.charCodeAt(0)).concat([0, 0, 1, 0, 0, 0, 0, 2, 0, 0, 0, 0]);
  const e = parseVivToc(new Uint8Array([...head, ...toc]));
  assert.deepEqual(e, [
    { name: names[0], offset: 100, size: 10 },
    { name: names[1], offset: 101, size: 11 },
  ]);
  assert.throws(() => parseVivToc(new Uint8Array(4)));
});

test("fe.txt audio keys", () => {
  const fe = "musiclevel = 127\r\nspeechlevel = 100\r\nnumplaylistsongs = 11\r\nplaylist = 0,1,2,3,4,5,6,7,8,9,10\r\n";
  assert.deepEqual(parseFeAudio(fe), {
    musicLevel: 127, speechLevel: 100, playlist: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10], randomPlaylist: false,
  });
  assert.deepEqual(parseFeAudio("").playlist, []);
});

test("countdown steps and digits follow Sched_Step", () => {
  assert.equal(START_SECS, 7);
  assert.equal(countdownStep(7), 0);
  assert.equal(countdownStep(3), 0x100); // "3"
  assert.equal(countdownStep(0), 0x1c0); // "Go"
  assert.equal(countdownDigit(5), null);
  assert.equal(countdownDigit(2.5), "3");
  assert.equal(countdownDigit(0.2), "1");
});

test("screen music: change fades out, same pattern continues, NONE stops", () => {
  const calls = [];
  const d = { stop: ms => calls.push(["stop", ms]), play: (p, m) => calls.push(["play", p, m]) };
  const s = new ScreenMusic();
  assert.equal(screenMusicPattern({ musicfile: "" }), "menu*");
  s.update("menu*", d);
  s.update("menu*", d);
  assert.deepEqual(calls, [["stop", 1500], ["play", "menu*", 0]]);
  calls.length = 0;
  s.update("NONE", d);
  assert.deepEqual(calls, [["stop", 1500], ["stop", 150]]);
});

test("volume ramps are linear", () => {
  const r = new VolumeRamp();
  r.set(79, 0);
  r.fade(500, 0, 1);
  assert.equal(r.value(1), 79);
  assert.equal(r.value(1.25), 40);
  assert.equal(r.value(2), 0);
});

test("race events: start, laps, best lap, finish", () => {
  const ev = new RaceEvents();
  assert.equal(ev.update({ phase: 1, lap: 1, best: 0 }).start, true);
  assert.equal(ev.update({ phase: 2, lap: 1, best: 0 }).lap, null);
  assert.deepEqual(ev.update({ phase: 2, lap: 2, best: 60 }).lap, { lapsDone: 1, best: false });
  assert.deepEqual(ev.update({ phase: 2, lap: 3, best: 55 }).lap, { lapsDone: 2, best: true });
  assert.deepEqual(ev.update({ phase: 2, lap: 4, best: 55 }).lap, { lapsDone: 3, best: false });
  assert.equal(ev.update({ phase: 4, lap: 4, best: 55 }).finished, true);
  assert.equal(ev.update({ phase: 5, lap: 4, best: 55 }).finished, false);
});

test("Go (step 0x1c0) is delivered on the countdown to racing transition", () => {
  assert.equal(announcerStep(1, 0, 3), 0x100);
  assert.equal(announcerStep(1, 1, 0.01), 0x1bf);
  assert.equal(announcerStep(2, 1, 0), 0x1c0);
  assert.equal(announcerStep(2, 2, 0), null);
  assert.equal(announcerStep(0, 0, 0), null);
});

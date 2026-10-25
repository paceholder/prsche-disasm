// Original music and race speech: browser helpers around the wasm MusicDirector /
// RaceAnnouncer (docs/re/music.md, docs/re/audio.md "race speech" in the research workspace).
// Pure functions here are tested by game-audio.test.mjs; Web Audio glue is in main.js.

// Sched_Step 0x414ff0: the start lasts 0x1c0 steps at 64 Hz.
export const SCHED_HZ = 64;
export const START_SECS = 0x1c0 / SCHED_HZ;
// FEScreen_UpdateMusic 0x4d1bd0.
export const SCREEN_FADE_MS = 1500;
export const NONE_FADE_MS = 150;
// Approximation A51: Music_Tick 0x4ad9c0 runs every 25 audio-library ticks (rate unknown).
export const MUSIC_TICK_MS = 100;

// BIGF table of contents (big-endian: magic, size, count, header size; then offset, size, name\0).
export function parseVivToc(bytes) {
  const d = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  const dv = new DataView(d.buffer, d.byteOffset, d.byteLength);
  if (d.length < 16 || String.fromCharCode(d[0], d[1], d[2], d[3]) !== "BIGF") throw new Error("not a BIGF archive");
  const count = dv.getUint32(8);
  const entries = [];
  let p = 16;
  for (let i = 0; i < count; i++) {
    if (p + 8 > d.length) throw new Error("viv TOC truncated");
    const offset = dv.getUint32(p), size = dv.getUint32(p + 4);
    p += 8;
    let e = p;
    while (e < d.length && d[e] !== 0) e++;
    if (e >= d.length) throw new Error("viv TOC truncated");
    entries.push({ name: String.fromCharCode(...d.subarray(p, e)), offset, size });
    p = e + 1;
  }
  return entries;
}

// fe.txt audio keys (data fe.txt:50-57); randomplaylist is absent in the shipped file (0).
export function parseFeAudio(text) {
  const get = key => {
    const m = new RegExp(`^\\s*${key}\\s*=\\s*([^\\r\\n]*)`, "mi").exec(text || "");
    return m ? m[1].trim() : null;
  };
  const int = (key, def) => {
    const v = parseInt(get(key) ?? "", 10);
    return Number.isFinite(v) ? v : def;
  };
  const count = int("numplaylistsongs", 0);
  const list = (get("playlist") || "").split(",").map(s => parseInt(s, 10)).filter(Number.isFinite);
  return {
    musicLevel: Math.max(0, Math.min(127, int("musiclevel", 127))),
    speechLevel: Math.max(0, Math.min(127, int("speechlevel", 127))),
    playlist: list.slice(0, count),
    randomPlaylist: int("randomplaylist", 0) !== 0,
  };
}

// Scheduler step of the start from the browser countdown (remaining seconds).
export function countdownStep(remaining, startSecs = START_SECS) {
  return Math.floor((startSecs - remaining) * SCHED_HZ);
}

// Scheduler step for the announcer this frame, or null. The browser phase switches to racing in the
// frame where the clock reaches 0, so the last step (0x1c0, "Go") is only seen as phase 2 right
// after phase 1: it is delivered on that transition (the original asks at step 0x1c0, 0x414ff0).
export function announcerStep(phase, prevPhase, remaining, startSecs = START_SECS) {
  if (phase === 1) return countdownStep(remaining, startSecs);
  if (phase === 2 && prevPhase === 1) return Math.round(startSecs * SCHED_HZ);
  return null;
}

// HUD digit: only the last 3 s show "3", "2", "1", in step with the phrases.
export function countdownDigit(remaining) {
  if (remaining > 3.0) return null;
  if (remaining > 2.0) return "3";
  if (remaining > 1.0) return "2";
  return "1";
}

// Layout musicfile, empty = "menu*" (0x4d1bd0, string 0x5d9184).
export function screenMusicPattern(attrs) {
  const p = attrs && typeof attrs.musicfile === "string" ? attrs.musicfile.trim() : "";
  return p || "menu*";
}

// FEScreen_UpdateMusic 0x4d1bd0 against a director (play/stop like MusicDirector).
export class ScreenMusic {
  constructor() { this.pattern = null; }
  reset() { this.pattern = null; }
  update(pattern, director) {
    if (pattern !== this.pattern) {
      director.stop(SCREEN_FADE_MS);
      if (pattern !== "NONE") director.play(pattern, 0);
      this.pattern = pattern;
    }
    if (pattern === "NONE") director.stop(NONE_FADE_MS);
  }
}

// Stream volume 0..127 with linear ramps (AudioStream_Fade 0x566ae0; 1/100 s steps).
export class VolumeRamp {
  constructor() { this.from = 0; this.to = 0; this.t0 = 0; this.dur = 0; }
  set(v, t) { this.from = this.to = v; this.t0 = t; this.dur = 0; }
  fade(ms, v, t) { this.from = this.value(t); this.to = v; this.t0 = t; this.dur = Math.max(0, ms) / 1000; }
  value(t) {
    if (this.dur <= 0 || t >= this.t0 + this.dur) return this.to;
    const k = Math.max(0, (t - this.t0) / this.dur);
    return Math.round(this.from + (this.to - this.from) * k);
  }
}

// Race events for the announcer from the HUD state polled each frame.
export class RaceEvents {
  constructor() { this.reset(); }
  reset() { this.phase = 0; this.lap = 1; this.best = 0; }
  // Returns { start, lap: {lapsDone, best} | null, finished, pause, resume }.
  update({ phase, lap, best }) {
    const out = { start: false, lap: null, finished: false, pause: false, resume: false };
    if (phase === 1 && this.phase !== 1) {
      out.start = true;
      this.lap = lap;
      this.best = 0;
    }
    if (phase === 2 && lap > this.lap) {
      const lapsDone = lap - 1;
      const newBest = lapsDone >= 2 && best > 0 && (this.best === 0 || best < this.best);
      out.lap = { lapsDone, best: newBest };
    }
    if ((phase === 4 || phase === 5) && this.phase !== 4 && this.phase !== 5 && this.phase > 0) out.finished = true;
    if (phase === 3 && this.phase !== 3) out.pause = true;
    if (phase !== 3 && this.phase === 3) out.resume = true;
    if (phase === 2 || phase === 1) { this.lap = lap; this.best = best; }
    this.phase = phase;
    return out;
  }
}

// The original race summary window `HRaceSummary` shown over the replay (docs/re/race-finish.md
// section 5). Model and geometry are pure (tested in race-summary.test.mjs); drawing uses the HUD
// fonts like the pause menu. 1 unit = 1 CSS px of the 3D view (the window base 0x41a510 draws at
// the render size). Recovered: window 500x325 (0x42bcd0), the column sets and widths
// (0x5b2e0c.. read from the exe), the 20 px rows and the table centring (0x42e8b0), the row
// contents (0x42c210), the strings (hud.loc 18100..18130), Space closes (0x42a3d0 -> 0x19).
// Approximation A292: colours, bars and fonts of the window and table (0x41a7a0/0x42e8b0 drawing
// not read) follow the reference video frame.
import { formatTicks } from "./race-hud-core.js";

export const WINDOW = Object.freeze({ w: 500, h: 325 }); // FUN_0042bcd0: 0x41a510(500, 0x145, ...)
export const ROW_H = 20; // 0x42e8b0: (rows + 1) * 0x14
export const ROWS = 8;

// hud.loc ids.
export const STR = Object.freeze({
  ESC: 18100, SPACE: 18101, NAME: 18111, CAR: 18112, BEST_LAP: 18113, TIME: 18114, POINTS: 18115,
  PLACE_1: 18116, SINGLE_RACE: 18129, TITLE: 18130,
});
const FALLBACK = {
  [STR.ESC]: "Esc to exit", [STR.SPACE]: "Spacebar to proceed", [STR.NAME]: "NAME", [STR.CAR]: "CAR",
  [STR.BEST_LAP]: "BEST LAP", [STR.TIME]: "TIME", [STR.POINTS]: "POINTS", [STR.SINGLE_RACE]: "Single Race",
  [STR.TITLE]: "Race Summary",
};
const PLACES = ["1st", "2nd", "3rd", "4th", "5th", "6th", "7th", "8th"];
// Column sets of 0x42bcd0 (widths at 0x5b2e0c / 0x5b2e1c, the first column has no header).
const LINEAR = { widths: [30, 90, 140, 80], heads: [null, STR.NAME, STR.CAR, STR.TIME] };
const CIRCUIT = { widths: [30, 90, 140, 100, 80], heads: [null, STR.NAME, STR.CAR, STR.BEST_LAP, STR.TIME] };
const DASHES = "---------------"; // 0x5cce0c: a time of 0

export function windowRect(W, H) {
  return { x: Math.trunc((W - WINDOW.w) / 2), y: Math.trunc((H - WINDOW.h) / 2), w: WINDOW.w, h: WINDOW.h };
}

// Table origin (0x42e8b0): centred horizontally on the column sum, vertically on the 9 rows.
export function tableRect(win, widths) {
  const w = widths.reduce((a, b) => a + b, 0);
  const h = (ROWS + 1) * ROW_H;
  return { x: Math.trunc((win.w - w) / 2) + win.x, y: ((win.h - h) >> 1) + win.y, w, h };
}

export function timeText(ticks) {
  return ticks > 0 ? formatTicks(ticks) : DASHES;
}

// results: viewer.race_results() JSON; info: { names: [player, ...opponents], car, colours: [0xRRGGBB] }.
// Rows in place order (0x42c110: row = place - 1).
export function summaryModel(results, info = {}, strings = {}) {
  const str = id => strings[String(id)] || FALLBACK[id] || "";
  const set = results.linear ? LINEAR : CIRCUIT;
  const rows = [];
  for (const c of results.cars) {
    const row = c.place - 1;
    if (row < 0 || row >= ROWS) continue;
    const cells = [
      strings[String(STR.PLACE_1 + row)] || PLACES[row],
      (info.names?.[c.index] ?? `Driver ${c.index + 1}`).slice(0, 31),
      (info.car ?? "").slice(0, 15),
    ];
    if (!results.linear) cells.push(timeText(c.best));
    cells.push(timeText(c.time));
    rows[row] = { cells, player: c.index === 0, colour: info.colours?.[c.index] ?? 0xffffff };
  }
  return {
    title: str(STR.TITLE),
    kind: str(STR.SINGLE_RACE),
    heads: set.heads.map(h => (h === null ? "" : str(h))),
    widths: set.widths,
    rows,
    footLeft: str(STR.ESC),
    footRight: str(STR.SPACE),
  };
}

// The window: closed, open. Space closes (0x42a3d0 -> command 0x19 -> close), Esc leaves the
// race (returns "exit").
export class RaceSummary {
  constructor() { this.model = null; this.W = 640; this.H = 480; }
  get isOpen() { return this.model !== null; }
  open(model) { this.model = model; }
  close() { this.model = null; }
  resize(W, H) { this.W = W; this.H = H; }
  key(code) {
    if (!this.isOpen) return null;
    if (code === "Space") { this.close(); return "close"; }
    if (code === "Escape") { this.close(); return "exit"; }
    return null;
  }
}

const css = c => `#${(c & 0xffffff).toString(16).padStart(6, "0")}`;

export function drawRaceSummary(ctx, summary, { fe = null, fonts = {}, drawText = null, textWidth = null } = {}) {
  const { W, H } = summary;
  ctx.clearRect(0, 0, W, H);
  const m = summary.model;
  if (!m) return;
  const win = windowRect(W, H);
  const text = (font, s, x, y, color, px, align = "left") => {
    if (font && drawText && fe) {
      const c = { a: 255, r: (color >>> 16) & 255, g: (color >>> 8) & 255, b: color & 255 };
      const w = textWidth(font, s);
      drawText(ctx, fe, font, s, Math.round(align === "right" ? x - w : x), y, c);
    } else {
      ctx.fillStyle = css(color);
      ctx.font = `${px}px sans-serif`;
      ctx.textAlign = align;
      ctx.textBaseline = "top";
      ctx.fillText(s, x, y);
    }
  };
  const bar = 20;
  // Body (A292: translucent gold over the replay) and the two bars.
  ctx.fillStyle = "rgba(236,196,112,0.62)";
  ctx.fillRect(win.x, win.y + bar, win.w, win.h - 2 * bar);
  ctx.fillStyle = "rgba(20,20,20,0.88)";
  ctx.fillRect(win.x, win.y, win.w, bar);
  ctx.fillRect(win.x, win.y + win.h - bar, win.w, bar);
  const font = fonts.list;
  text(font, m.kind, win.x + 10, win.y + 4, 0xdddddd, 12);
  text(font, m.title, win.x + win.w - 10, win.y + 4, 0xdddddd, 12, "right");
  text(font, m.footLeft, win.x + 10, win.y + win.h - bar + 4, 0xdddddd, 12);
  text(font, m.footRight, win.x + win.w - 10, win.y + win.h - bar + 4, 0xdddddd, 12, "right");
  // Table: header row, 8 rows, column lines.
  const t = tableRect(win, m.widths);
  ctx.fillStyle = "#f4a400";
  ctx.fillRect(t.x, t.y, t.w, ROW_H);
  const xs = [t.x];
  for (const w of m.widths) xs.push(xs[xs.length - 1] + w);
  m.heads.forEach((h, i) => text(font, h, xs[i] + 4, t.y + 4, 0x000000, 12));
  m.rows.forEach((r, i) => {
    if (!r) return;
    const y = t.y + ROW_H * (i + 1);
    if (r.player) {
      ctx.fillStyle = "rgba(240,190,90,0.95)";
      ctx.fillRect(t.x, y, t.w, ROW_H);
    }
    r.cells.forEach((s, k) => {
      let x = xs[k] + 4;
      if (k === 2) {
        // Colour swatch of the car (res+0x218) before the car name.
        ctx.fillStyle = css(r.colour);
        ctx.fillRect(x, y + 6, 18, 8);
        ctx.strokeStyle = "#000000";
        ctx.strokeRect(x + 0.5, y + 6.5, 17, 7);
        x += 22;
      }
      text(font, s, x, y + 4, 0x101010, 12);
    });
  });
  ctx.strokeStyle = "#202020";
  ctx.lineWidth = 1;
  ctx.strokeRect(t.x + 0.5, t.y + 0.5, t.w - 1, t.h - 1);
  for (let i = 1; i < xs.length - 1; i++) {
    ctx.beginPath();
    ctx.moveTo(xs[i] + 0.5, t.y + ROW_H);
    ctx.lineTo(xs[i] + 0.5, t.y + t.h);
    ctx.stroke();
  }
  ctx.beginPath();
  ctx.moveTo(t.x, t.y + ROW_H + 0.5);
  ctx.lineTo(t.x + t.w, t.y + ROW_H + 0.5);
  ctx.stroke();
}

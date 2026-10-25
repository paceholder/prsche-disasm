// Original in-race pause menu and its exit menu (docs/re/pause.md).
// Model and geometry are pure (tested in pause-menu.test.mjs); drawing uses the HUD fonts.
// 1 unit = 1 CSS px of the 3D view, as the original uses its render size directly.
// Deviations (keyboard, no "view replay", panel look): docs/approximations.md A150, A151.

export const CODE = Object.freeze({
  CONTINUE: 2, RACE_AGAIN: 3, QUIT: 0x1a, // pause menu (PauseMenu_Handle 0x42a6a0)
  VIEW_REPLAY: 0x15, EXIT_WINDOWS: 4, EXIT_GAME_MENU: 5, CANCEL: 0x18, // exit menu (0x421b70)
});

// hud.loc ids (hexadecimal in the exe: 0x46d0 ...).
export const STR = Object.freeze({
  PAUSE_TITLE: 0x46d0, CONTINUE: 0x46bc, RACE_AGAIN: 0x46bd, QUIT: 0x46be,
  EXIT_TITLE: 0x46ba, VIEW_REPLAY: 0x46b6, EXIT_WINDOWS: 0x46b7, EXIT_GAME_MENU: 0x46b8, CANCEL: 0x46b9,
});
const FALLBACK = {
  [STR.PAUSE_TITLE]: "Pause Menu", [STR.CONTINUE]: "continue", [STR.RACE_AGAIN]: "race again",
  [STR.QUIT]: "quit…", [STR.EXIT_TITLE]: "exit", [STR.VIEW_REPLAY]: "view replay",
  [STR.EXIT_WINDOWS]: "exit to windows", [STR.EXIT_GAME_MENU]: "exit to game menu", [STR.CANCEL]: "cancel",
};

export const WINDOW = Object.freeze({ w: 500, h: 325 }); // 0x5b2d40/44, 0x5b2af4
export const BUTTON = Object.freeze({ w: 142, h: 19 }); // 0x5b2d48/4c
const STEP = BUTTON.h * 1.6; // fmul by the double 1.6 at 0x5b2b00

// Window rectangle, centred (0x41ae20, 0x41b100).
export function windowRect(W, H) {
  return { x: Math.trunc((W - WINDOW.w) / 2), y: Math.trunc((H - WINDOW.h) / 2), w: WINDOW.w, h: WINDOW.h };
}

// Button y positions of `count` buttons: first = trunc(H/2 - STEP*count*0.5), next = trunc(prev + STEP)
// (the pause menu has 3, the exit menu 4). H/2 is the integer half of the height.
export function buttonYs(H, count) {
  const ys = [Math.trunc(Math.trunc(H / 2) - STEP * count * 0.5)];
  for (let i = 1; i < count; i++) ys.push(Math.trunc(ys[i - 1] + STEP));
  return ys;
}

export function buttonX(W) { return Math.trunc(W / 2) - Math.trunc(BUTTON.w / 2); }

const PAUSE_ITEMS = [
  { str: STR.CONTINUE, code: CODE.CONTINUE },
  { str: STR.RACE_AGAIN, code: CODE.RACE_AGAIN },
  { str: STR.QUIT, code: CODE.QUIT },
];
// Creation order of the exit menu: view replay, exit to windows, exit to game menu, cancel.
const EXIT_ITEMS = [
  { str: STR.VIEW_REPLAY, code: CODE.VIEW_REPLAY },
  { str: STR.EXIT_WINDOWS, code: CODE.EXIT_WINDOWS },
  { str: STR.EXIT_GAME_MENU, code: CODE.EXIT_GAME_MENU },
  { str: STR.CANCEL, code: CODE.CANCEL },
];

export class PauseMenu {
  // opts.viewReplay: offer "view replay" (the engine has no replay: A151).
  constructor({ strings = {}, viewReplay = false } = {}) {
    this.strings = strings;
    this.viewReplay = viewReplay;
    this.kind = null; // null (closed) | "pause" | "exit"
    this.hot = -1;
    this.W = 640;
    this.H = 480;
  }

  get isOpen() { return this.kind !== null; }
  str(id) { return this.strings[String(id)] || FALLBACK[id] || ""; }
  title() { return this.str(this.kind === "exit" ? STR.EXIT_TITLE : STR.PAUSE_TITLE); }

  open() { this.kind = "pause"; this.hot = -1; }
  close() { this.kind = null; this.hot = -1; }
  resize(W, H) { this.W = W; this.H = H; }

  // Visible buttons: { label, code, x, y, w, h }.
  buttons() {
    if (!this.kind) return [];
    const all = this.kind === "pause" ? PAUSE_ITEMS : EXIT_ITEMS;
    const ys = buttonYs(this.H, all.length);
    const x = buttonX(this.W);
    const out = [];
    all.forEach((it, i) => {
      if (it.code === CODE.VIEW_REPLAY && !this.viewReplay) return;
      out.push({ label: this.str(it.str), code: it.code, x, y: ys[i], w: BUTTON.w, h: BUTTON.h });
    });
    return out;
  }

  hit(px, py) {
    return this.buttons().findIndex(b => px >= b.x && px < b.x + b.w && py >= b.y && py < b.y + b.h);
  }

  pointerMove(px, py) { this.hot = this.hit(px, py); return this.hot; }

  // A click on a hot button returns its code (RaceButton_Click 0x41b300 + the handler).
  click(px, py) {
    const i = this.hit(px, py);
    this.hot = i;
    return i < 0 ? null : this.#activate(this.buttons()[i].code);
  }

  // Keyboard (A150): Up/Down move, Enter presses, Esc closes as continue (cancel in the exit menu).
  key(code) {
    const n = this.buttons().length;
    if (!n) return null;
    if (code === "ArrowDown") this.hot = (this.hot + 1) % n;
    else if (code === "ArrowUp") this.hot = this.hot <= 0 ? n - 1 : this.hot - 1;
    else if (code === "Enter" || code === "NumpadEnter") {
      return this.hot < 0 ? null : this.#activate(this.buttons()[this.hot].code);
    } else if (code === "Escape") {
      return this.#activate(this.kind === "exit" ? CODE.CANCEL : CODE.CONTINUE);
    }
    return null;
  }

  // Menu-stack reaction (RaceMenus_Update 0x425ef0): quit... shows the exit menu, cancel returns.
  // Every code is returned so that the caller reacts (continue, race again, exits).
  #activate(code) {
    if (code === CODE.QUIT) { this.kind = "exit"; this.hot = -1; }
    else if (code === CODE.CANCEL) { this.kind = "pause"; this.hot = -1; }
    return code;
  }
}

// Drawing. fe = { assets }, fonts = { big, list } from RaceHud; plain canvas text when fonts are missing.
export function drawPauseMenu(ctx, menu, { fe = null, fonts = {}, drawText = null, textWidth = null } = {}) {
  const { W, H } = menu;
  ctx.clearRect(0, 0, W, H);
  if (!menu.isOpen) return;
  const win = windowRect(W, H);
  ctx.fillStyle = "rgba(0,0,0,0.72)";
  ctx.fillRect(win.x, win.y, win.w, win.h);
  ctx.strokeStyle = "rgba(255,255,255,0.7)";
  ctx.lineWidth = 1;
  ctx.strokeRect(win.x + 0.5, win.y + 0.5, win.w - 1, win.h - 1);
  const text = (font, s, cx, y, color, px) => {
    if (font && drawText && fe) {
      const c = { a: (color >>> 24) & 255, r: (color >>> 16) & 255, g: (color >>> 8) & 255, b: color & 255 };
      drawText(ctx, fe, font, s, Math.round(cx - textWidth(font, s) / 2), y, c);
    } else {
      ctx.fillStyle = `#${(color & 0xffffff).toString(16).padStart(6, "0")}`;
      ctx.font = `${px}px sans-serif`;
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      ctx.fillText(s, cx, y);
    }
  };
  text(fonts.big, menu.title(), W / 2, win.y + 16, 0xffffffff, 20);
  menu.buttons().forEach((b, i) => {
    const hot = i === menu.hot;
    ctx.fillStyle = hot ? "#ffc61a" : "rgba(40,40,40,0.85)";
    ctx.fillRect(b.x, b.y, b.w, b.h);
    ctx.strokeStyle = hot ? "#ffffff" : "rgba(255,255,255,0.5)";
    ctx.strokeRect(b.x + 0.5, b.y + 0.5, b.w - 1, b.h - 1);
    text(fonts.list, b.label, b.x + b.w / 2, b.y + 3, hot ? 0xff000000 : 0xffffffff, 13);
  });
}

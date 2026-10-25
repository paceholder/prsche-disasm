// Original front end: screens built from FEData layouts, drawn on a 640x480 canvas over the
// 3D view (the room). Evidence and control semantics: docs/re/frontend.md (workspace root).
import {
  FE_WIDTH, FE_HEIGHT, backdropLayoutName, attrInt, attrPair, parseColor, cssColor,
  fitViewport, toLayoutPoint, inRect, transitionState, nextLevels, backLevels,
} from "./fe-core.js";
import { FeAssets } from "./fe-assets.js";

const TRANSITION_DELAY_MS = 3 * 1000 / 60;

export class FrontEnd {
  // host: { api (wasm fe_* functions), readFile(path), onAction(name, control) -> handled?,
  //         onRoom(roomId, carAccessor|null), accessor(name) -> string, requestFrame() }
  constructor(canvas, host) {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d");
    this.host = host;
    this.assets = new FeAssets(host.api, host.readFile);
    this.levels = [];
    this.popups = [];
    this.screen = null;
    this.hover = null;
    this.pressed = null;
    this.visible = false;
    this.view = fitViewport(1, 1);
    canvas.addEventListener("pointermove", e => this.pointer("move", e));
    canvas.addEventListener("pointerdown", e => this.pointer("down", e));
    canvas.addEventListener("pointerup", e => this.pointer("up", e));
    canvas.addEventListener("pointerleave", () => { this.hover = null; });
    canvas.addEventListener("wheel", e => this.wheel(e), { passive: true });
    this.logoPacks = new Map();
  }

  async init() {
    await this.assets.loadStrings(0);
    this.bottombar = await this.prepare("bottombar");
  }

  // Load a layout and everything it draws (art, fonts) so that drawing never waits.
  async prepare(name) {
    const sections = await this.assets.layout(name);
    if (!sections) return null;
    const screenAttrs = sections.find(s => s.index === 0)?.attrs ?? {};
    // Section 0 only carries screen attributes; it is never drawn [VA 0x5284a0].
    const controls = sections.filter(s => s.index !== 0);
    for (const c of controls) {
      const a = c.attrs;
      if (a.shapepathname) c.shapes = await this.assets.pack(a.shapepathname);
      if (c.type === "COMBOBOXSLIDER" || c.type === "COLORBOX") c.arrow = await this.assets.pack("weeicons.fsh");
      if (c.type === "EXTRACIRCLE") c.extra = await this.assets.pack("extracircle.fsh");
      if (c.type === "CARLOGO") await this.loadLogoPack(this.host.carLogo?.(a.accessor)?.pack);
      c.fonts = {};
      for (const font of CONTROL_FONTS[c.type] ?? []) c.fonts[font] = await this.assets.font(font);
      if (a.fontpathname) c.fonts.own = await this.assets.font(a.fontpathname);
    }
    return { name, screenAttrs, controls };
  }

  // Car logo packs (finallogos1/2) are per car, loaded when a car first needs them.
  async loadLogoPack(pack) {
    if (!pack) return;
    const key = pack.toLowerCase();
    if (this.logoPacks.has(key)) return;
    this.logoPacks.set(key, null);
    this.logoPacks.set(key, await this.assets.pack(`${pack}.fsh`));
    this.host.requestFrame?.();
  }

  closeList() {
    const c = this.openList;
    if (c?.list && !c.list.closing) {
      c.list.closing = true;
      c.list.opened = performance.now();
    }
    this.openList = null;
  }

  // Build the shown screens from the levels: level 0 is the screen (room, backdrop and
  // callback list come from it [VA 0x4d2570]), higher levels are pop-ups over it.
  async refresh() {
    this.closeList();
    const names = this.levels.map(level => level[level.length - 1]);
    const layouts = [];
    for (const name of names) {
      const layout = this.cache?.get(name) ?? await this.prepare(name);
      if (!layout) return false;
      layouts.push(layout);
    }
    this.cache = new Map(layouts.map(l => [l.name, l]));
    const base = layouts[0];
    if (!base.backdrop) {
      const backdropName = backdropLayoutName(base.screenAttrs.screen || "ID_SCREEN");
      base.backdrop = backdropName ? await this.prepare(backdropName) : null;
    }
    const now = performance.now();
    const previous = [this.screen, ...(this.popups ?? [])];
    for (const l of layouts) if (!previous.includes(l)) l.started = now;
    this.screen = base;
    this.popups = layouts.slice(1);
    this.loading = null;
    this.hover = null;
    this.visible = true;
    const car = base.controls.find(c => c.type === "3DCAR");
    await this.host.onRoom?.(base.screenAttrs.room || "ID_TESTROOM", car ? (car.attrs.accessor || "zMergedList.Car") : null);
    this.host.requestFrame?.();
    return true;
  }

  // The screen whose controls take input: the top pop-up, else the screen.
  get top() {
    return this.popups?.length ? this.popups[this.popups.length - 1] : this.screen;
  }

  async goto(name, offset = 0) {
    this.levels = nextLevels(this.levels ?? [], name.toLowerCase(), offset);
    return this.refresh();
  }

  async back() {
    const levels = backLevels(this.levels ?? []);
    if (!levels) return false;
    this.levels = levels;
    return this.refresh();
  }

  async start(name = "gamesetup") {
    this.levels = [];
    this.cache = null;
    this.screen = null;
    this.popups = [];
    return this.goto(name, 1);
  }

  // Loading screen [VA 0x4a4a70]: `era<N>.fsh` (start-up: `loadscrn.fsh` [VA 0x4dd600]) or,
  // while a race loads, `era<N>tr.fsh` plus the track picture from FEData/TrackArt.
  // N = 0 outside Evolution, 1..3 for Classic/Golden/Modern in Evolution.
  async showLoading({ pack = "loadscrn.fsh", track = null, reverse = false } = {}) {
    const tiles = await this.assets.pack(pack);
    const art = track ? await this.assets.pack(`${track}.fsh`, "FEData/TrackArt") : null;
    this.loading = { tiles, art, reverse };
    this.visible = true;
    this.renderLoading();
  }

  renderLoading() {
    const { ctx, view, loading } = this;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, this.canvas.width, this.canvas.height);
    ctx.setTransform(view.scale, 0, 0, view.scale, view.x, view.y);
    for (const [i, x, y] of loadingTileLayout()) drawShape(ctx, loading.tiles?.[i], x, y);
    // Track picture and its name at |+8|,|+10| of their shape headers; then the start/finish
    // marker, shape 3 when the flag at 0x657430 is set (Hypothesis: reverse), else 2.
    const art = loading.art;
    if (art) {
      for (const i of [0, 1, loading.reverse ? 3 : 2]) {
        const shape = art[i];
        if (shape) drawShape(ctx, shape, Math.abs(shape.cx), Math.abs(shape.cy));
      }
    }
  }

  hideLoading() {
    this.loading = null;
    this.hide();
  }

  hide() {
    this.visible = false;
    this.loading = null;
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
  }

  resize(width, height) {
    if (this.canvas.width !== width || this.canvas.height !== height) {
      this.canvas.width = width;
      this.canvas.height = height;
    }
    this.view = fitViewport(width, height);
  }

  // All drawn layers of the current screen, back to front [VA 0x4d2f30]:
  // backdrop layout, the screen's layout, the bottom bar. Within a layout the last section
  // is drawn first [VA 0x528760].
  layers() {
    if (!this.screen) return [];
    const out = [];
    for (const layout of [this.screen.backdrop, this.screen, this.bottombar, ...(this.popups ?? [])]) {
      if (!layout) continue;
      for (let i = layout.controls.length - 1; i >= 0; i--) {
        const control = layout.controls[i];
        // Controls shown/hidden by the callback list (e.g. FECallBackList update 0x50cc30).
        if (this.host.controlVisible?.(control.attrs.name, control) === false) continue;
        out.push({ layout, control });
      }
    }
    return out;
  }

  render(now = performance.now()) {
    if (!this.visible) return;
    if (this.loading) return this.renderLoading();
    const { ctx, view } = this;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    // Bars outside the 4:3 frame stay transparent so the room shows at the sides (A14).
    ctx.setTransform(view.scale, 0, 0, view.scale, view.x, view.y);
    ctx.imageSmoothingEnabled = view.scale % 1 !== 0;
    const elapsed = now - (this.screen?.started ?? now) - TRANSITION_DELAY_MS;
    for (const { layout, control } of this.layers()) {
      const draw = CONTROL_DRAW[control.type];
      if (!draw) continue;
      const fromScreen = layout === this.screen || this.popups?.includes(layout);
      const t = transitionState(fromScreen ? control.attrs.transon : "FADE", elapsed);
      ctx.save();
      ctx.globalAlpha = t.alpha;
      ctx.translate(t.dx, t.dy);
      draw(ctx, control, this, now);
      ctx.restore();
    }
    // Open (or closing) drop-down lists on top.
    for (const { control } of this.layers()) {
      if (control.list) drawComboList(ctx, control, this, now);
    }
    if (elapsed < 600) this.host.requestFrame?.();
  }

  // Interactive controls in hit-test order (front first).
  hitTest(x, y) {
    const layers = this.layers();
    for (let i = layers.length - 1; i >= 0; i--) {
      const { control, layout } = layers[i];
      // Only the top screen (a pop-up when one is open) and the bottom bar take input.
      if (layout !== this.top && layout !== this.bottombar) continue;
      const rect = CONTROL_HIT[control.type]?.(control, this, layout);
      if (rect && inRect(rect, x, y) && !this.disabled(control, layout)) return control;
    }
    return null;
  }

  disabled(control, layout) {
    // RACE follows canplaygame of the screen: 0 off, 1 on [VA 0x4d8250].
    if (control.type === "BOTTOMBARBUTTON" && control.attrs.callback === "PLAYGAME") {
      return attrInt(this.screen?.screenAttrs ?? {}, "canplaygame", 0) !== 1;
    }
    if (control.attrs.state === "disabled") return true;
    return layout === this.screen?.backdrop;
  }

  pointer(type, event) {
    if (!this.visible) return;
    const rect = this.canvas.getBoundingClientRect();
    const scale = this.canvas.width / Math.max(rect.width, 1);
    const [x, y] = toLayoutPoint(this.view, (event.clientX - rect.left) * scale, (event.clientY - rect.top) * scale);
    // An open list takes the pointer first [VA 0x4c6af0].
    if (this.openList) {
      const c = this.openList;
      const L = comboList(c, this, performance.now());
      const inList = L && x >= L.x && x < L.x + L.w && y >= L.y && y < L.y + L.full;
      const row = inList ? L.scroll + Math.floor((y - L.y) / LIST_ROW_H) : -1;
      if (type === "move") c.list.hoverRow = row < L.items.length ? row : -1;
      if (type === "up") {
        if (inList && row < L.items.length) {
          L.acc?.select?.(row);
          this.host.onChange?.(c.attrs.accessor, c);
          this.closeList();
        } else {
          const g = comboGeometry(c, this);
          const onBar = inRect({ x: g.x, y: g.y, w: g.w, h: g.h }, x, y);
          this.closeList();
          if (onBar) { this.host.requestFrame?.(); return; }
        }
      }
      if (type !== "up") { this.host.requestFrame?.(); return; }
    }
    const hit = this.hitTest(x, y);
    if (type === "move") {
      if (hit !== this.hover) { this.hover = hit; this.hoverStarted = performance.now(); }
    } else if (type === "down") {
      this.pressed = hit;
    } else if (type === "up") {
      if (hit && hit === this.pressed) this.activate(hit, x);
      this.pressed = null;
    }
    this.canvas.style.cursor = hit ? "pointer" : "default";
    this.host.requestFrame?.();
  }

  wheel(event) {
    const c = this.openList;
    if (!c?.list) return;
    const L = comboList(c, this, performance.now());
    const rows = Math.floor(L.full / LIST_ROW_H);
    c.list.scroll = Math.max(0, Math.min(L.items.length - rows, (c.list.scroll ?? 0) + Math.sign(event.deltaY)));
    this.host.requestFrame?.();
  }

  // ComboBox click [VA 0x4c6af0]: on the label part run callback/destination, else step the
  // accessor; on the value part open the list (0.125 s), scrolled to the current item.
  comboClick(control, x) {
    const g = comboGeometry(control, this);
    const a = control.attrs;
    const acc = this.host.accessor?.(a.accessor);
    if (g.labelW && x < g.x + g.labelW) {
      if (a.callback || a.destination) return false;
      acc?.step?.(1);
      this.host.onChange?.(a.accessor, control);
      return true;
    }
    const items = acc?.items?.() ?? [];
    const rows = Math.floor(Math.min(items.length * LIST_ROW_H, attrInt(a, "selectionboxmaxheight", 160)) / LIST_ROW_H);
    const current = Math.max(0, acc?.index?.() ?? 0);
    control.list = { opened: performance.now(), closing: false, hoverRow: -1,
      scroll: Math.max(0, Math.min(items.length - rows, current - Math.floor(rows / 2))) };
    this.openList = control;
    return true;
  }

  // GOTOBUTTON: callback first, then destination with its stack offset [VA 0x4c9470].
  async activate(control, x = 0) {
    const a = control.attrs;
    if ((control.type === "COMBOBOXSLIDER" || control.type === "COLORBOX") && this.comboClick(control, x)) return;
    const callback = (a.callback || "").toUpperCase();
    if (callback === "GOBACK") return this.back();
    if (callback && this.host.onAction?.(callback, control, this) === true) return;
    if (a.destination) {
      const dest = a.destination.toLowerCase();
      if (this.host.onAction?.(`GOTO:${dest}`, control, this) === true) return;
      await this.goto(dest, attrInt(a, "offset", 0));
    }
  }
}

// 45 loading-screen tiles over 640x480: 20 of 32 px, 10 of 64 px, 3 rows of 5 of 128 px
// [VA 0x4a4a70; data Art/era*.fsh, loadscrn.fsh].
export function loadingTileLayout() {
  const out = [];
  let i = 0;
  for (let x = 0; x < 640; x += 32) out.push([i++, x, 0]);
  for (let x = 0; x < 640; x += 64) out.push([i++, x, 32]);
  for (let y = 96; y < 480; y += 128) for (let x = 0; x < 640; x += 128) out.push([i++, x, y]);
  return out;
}

// ---- drawing ---------------------------------------------------------------------------

export function textWidth(font, text) {
  if (!font) return 0;
  let w = 0;
  for (const ch of text) w += (font.glyphs.get(ch.charCodeAt(0)) ?? font.glyphs.get(32))?.advance ?? 0;
  return w;
}

export function drawText(ctx, fe, font, text, x, y, color) {
  if (!font || !text) return;
  const atlas = fe.assets.tintedAtlas(font, cssColor(color));
  let pen = x;
  for (const ch of text) {
    const g = font.glyphs.get(ch.charCodeAt(0)) ?? font.glyphs.get(32);
    if (!g) continue;
    if (g.w > 0 && g.h > 0 && ch !== " ") ctx.drawImage(atlas, g.x, g.y, g.w, g.h, pen + g.xo, y + g.yo, g.w, g.h);
    pen += g.advance;
  }
}

// PICTURE: shape `shapeIndex` at `pos`, native size; flip bit 0 mirrors x, bit 1 mirrors y.
function drawShape(ctx, shape, x, y, flip = 0) {
  if (!shape?.canvas) return;
  ctx.save();
  ctx.translate(x + (flip & 1 ? shape.width : 0), y + (flip & 2 ? shape.height : 0));
  ctx.scale(flip & 1 ? -1 : 1, flip & 2 ? -1 : 1);
  ctx.drawImage(shape.canvas, 0, 0);
  ctx.restore();
}

function drawPicture(ctx, c) {
  const [x, y] = attrPair(c.attrs, "pos");
  drawShape(ctx, c.shapes?.[attrInt(c.attrs, "shapeindex", 0)], x, y, attrInt(c.attrs, "flip", 0));
}

function buttonState(c, fe, layout) {
  if (fe.disabled(c, layout ?? fe.screen)) return "disabled";
  if (fe.hover === c) return "hover";
  return "normal";
}

// GOTOBUTTON [data GoToButton.ctr; VA 0x4c90c0].
function drawGotoButton(ctx, c, fe) {
  const a = c.attrs;
  const [x, y] = attrPair(a, "pos");
  const w = attrInt(a, "width", 157);
  const h = attrInt(a, "height", 18);
  const state = buttonState(c, fe);
  const color = parseColor(state === "hover" ? a.hovercolor : state === "disabled" ? a.disabledcolor : a.maincolor,
    state === "hover" ? 0xffffbb00 : state === "disabled" ? 0x30ff0000 : 0xffbc0000);
  if (attrInt(a, "dropshadow", 1) === 1) {
    ctx.fillStyle = "rgba(0,0,0,0.5)"; // shadow box offset 4,4 [VA 0x520f00]; colour: A17
    ctx.fillRect(x + 4, y + 4, w, h);
  }
  // Stems: 1 px lines at the left/right edge from the top, stem + height long [VA 0x4c90c0].
  ctx.fillStyle = cssColor(color);
  const left = attrInt(a, "stemleftheight", 0);
  const right = attrInt(a, "stemrightheight", 0);
  if (left > 0) ctx.fillRect(x, y, 1, left + h);
  if (right > 0) ctx.fillRect(x + w - 1, y, 1, right + h);
  ctx.fillRect(x + 1, y + 1, w - 2, h - 2);
  // Glint (glintybox): a lighter top band stands in for the glint animator (A17).
  if (state !== "disabled") {
    const glint = ctx.createLinearGradient(0, y + 1, 0, y + h - 1);
    glint.addColorStop(0, "rgba(255,255,255,0.35)");
    glint.addColorStop(0.5, "rgba(255,255,255,0.05)");
    glint.addColorStop(1, "rgba(0,0,0,0.15)");
    ctx.fillStyle = glint;
    ctx.fillRect(x + 1, y + 1, w - 2, h - 2);
  }
  const medium = attrInt(a, "medium", 0);
  const text = fe.assets.string(a.stringid);
  const white = parseColor(state === "disabled" ? "0x30ffffff" : "0xffffffff");
  if (medium === 0) drawText(ctx, fe, c.fonts["UniversC11b.ffn"], text, x + 5, y + 2, white);
  else if (medium & 1) {
    const f = c.fonts["UniversC11m.ffn"];
    drawText(ctx, fe, f, text, x + Math.floor((w - textWidth(f, text)) / 2), y + 2, white);
  } else {
    const f = c.fonts["Univers8b.ffn"];
    drawText(ctx, fe, f, text, medium & 4 ? x + 5 : x + Math.floor((w - textWidth(f, text)) / 2), y + 2, white);
  }
  return { w, h, x, y };
}

// ---- ComboBoxSlider / ColorBox (FEComboBox, docs/re/frontend.md "Drop-down lists") ----
// host.accessor(name) returns {text(), items() -> [string], index(), select(i), step(d),
// color?() -> css} or null.

const LIST_OPEN_MS = 16 * 1000 / 128;   // 16 ticks at 128 Hz [VA 0x4c5460]
const LIST_ROW_H = 13;                   // Univers8b line height (Hypothesis)

function comboGeometry(c, fe) {
  const a = c.attrs;
  const [x, y] = attrPair(a, "pos");
  const w = attrInt(a, "width", 100);
  const h = attrInt(a, "height", 18);
  const small = attrInt(a, "usesmallfont", 0) === 1;
  const labelFont = c.fonts[small ? "Univers8b.ffn" : "UniversC11b.ffn"];
  const label = attrInt(a, "nolabel", 0) & 1 ? "" : fe.assets.string(a.stringid);
  // Divider at the end of the label (+0x1f0): label width plus the 5 px inset each side
  // (Hypothesis).
  const labelW = label ? 5 + textWidth(labelFont, label) + 5 : 0;
  return { x, y, w, h, label, labelFont, labelW };
}

function comboList(c, fe, now) {
  const st = c.list;
  if (!st) return null;
  const g = comboGeometry(c, fe);
  const acc = fe.host.accessor?.(c.attrs.accessor);
  const items = acc?.items?.() ?? [];
  const t = Math.min(1, Math.max(0, (now - st.opened) / LIST_OPEN_MS));
  const k = st.closing ? 1 - t : t;
  const full = Math.min(items.length * LIST_ROW_H, attrInt(c.attrs, "selectionboxmaxheight", 160));
  const boxW = attrInt(c.attrs, "selectionboxwidth", 0) || g.w;
  const dir = c.attrs.selectionboxslidedirection === "" ? 0 : attrInt(c.attrs, "selectionboxslidedirection", -1);
  const w = dir === 0 ? g.w : g.w + (boxW - g.w) * k;
  const x = dir < 0 ? g.x + g.w - w : g.x;
  return { x, y: g.y + g.h, w, h: full * k, full, items, acc, k, scroll: st.scroll ?? 0 };
}

function drawComboBox(ctx, c, fe, now) {
  const a = c.attrs;
  const g = comboGeometry(c, fe);
  const { x, y, w, h } = g;
  const disabled = fe.disabled(c, fe.screen);
  const open = Boolean(c.list);
  const hover = !disabled && (fe.hover === c || open);
  // Bar colours [VA 0x4c5810, 0x4d87b0, 0x4d87c0].
  const bar = parseColor(disabled ? "0x30ff0000" : hover ? "0xffffbb00" : "0xffbc0000");
  if (attrInt(a, "dropshadow", 1) === 1) {
    ctx.fillStyle = "rgba(0,0,0,0.5)"; // A17
    ctx.fillRect(x + 4, y + 4, w, h);
  }
  ctx.fillStyle = "#000"; // stems are black [VA 0x4d87e0]
  const left = attrInt(a, "stemleftheight", 0);
  const right = attrInt(a, "stemrightheight", 0);
  if (left > 0) ctx.fillRect(x, y, 1, h + left);
  if (right > 0) ctx.fillRect(x + w, y, 1, h + right);
  ctx.fillStyle = cssColor(bar);
  ctx.fillRect(x, y, w, h);
  if (!hover) {
    // Shading pass when not hovered [VA 0x521810]: a gradient stands in (A17).
    const shade = ctx.createLinearGradient(0, y, 0, y + h);
    shade.addColorStop(0, "rgba(255,255,255,0.30)");
    shade.addColorStop(0.5, "rgba(255,255,255,0.04)");
    shade.addColorStop(1, "rgba(0,0,0,0.18)");
    ctx.fillStyle = shade;
    ctx.fillRect(x, y, w, h);
  }
  ctx.strokeStyle = "#000";
  ctx.lineWidth = 1;
  ctx.strokeRect(x + 0.5, y + 0.5, w - 1, h - 1);
  if (g.labelW) ctx.fillRect(x + g.labelW, y, 1, h);
  drawText(ctx, fe, g.labelFont, g.label, x + 5, y + 2, parseColor("0xffffffff"));
  const acc = fe.host.accessor?.(a.accessor);
  const valueFont = c.fonts["UniversC11m.ffn"];
  const value = acc?.text?.() ?? "";
  const valueColor = parseColor(disabled ? "0x30000000" : "0xff000000");
  drawText(ctx, fe, valueFont, value, x + w - 12 - textWidth(valueFont, value) - (c.type === "COLORBOX" ? 30 : 0), y + 2, valueColor);
  if (c.type === "COLORBOX" && acc?.color) {
    ctx.fillStyle = acc.color();
    ctx.fillRect(x + w - 40, y + 4, 20, 10);
    ctx.strokeRect(x + w - 40 + 0.5, y + 4 + 0.5, 19, 9);
  }
  const arrow = c.arrow?.[14]; // weeicons.fsh shape 14 "bard"
  if (arrow) drawShape(ctx, arrow, x + w - arrow.width - 3, y + Math.floor((h - arrow.height) / 2) + 1);
}

// The open list, drawn above everything (the control is brought to front, VA 0x528850).
function drawComboList(ctx, c, fe, now) {
  const L = comboList(c, fe, now);
  if (!L || L.h < 1) {
    if (c.list?.closing) c.list = null;
    return;
  }
  // Gouraud fill: 0xFFFFBB00 at the top to a blend with 0xB282A8B0 by height [VA 0x4d96b0].
  const grad = ctx.createLinearGradient(0, L.y, 0, L.y + L.h);
  grad.addColorStop(0, "rgba(255,187,0,1)");
  const m = L.full > 0 ? L.h / L.full : 0;
  grad.addColorStop(1, `rgba(${Math.round(255 + (130 - 255) * m)},${Math.round(187 + (168 - 187) * m)},${Math.round(0 + 176 * m)},${(1 + (0xb2 / 255 - 1) * m).toFixed(3)})`);
  ctx.fillStyle = grad;
  ctx.fillRect(L.x, L.y, L.w, L.h);
  ctx.save();
  ctx.beginPath();
  ctx.rect(L.x, L.y, L.w, L.h);
  ctx.clip();
  const font = c.fonts["Univers8b.ffn"];
  const current = L.acc?.index?.() ?? -1;
  for (let i = L.scroll, row = 0; i < L.items.length; i++, row++) {
    const ry = L.y + row * LIST_ROW_H;
    if (ry > L.y + L.h) break;
    const hovered = c.list && !c.list.closing && c.list.hoverRow === i;
    if (hovered) {
      ctx.fillStyle = "rgba(188,0,0,1)";
      ctx.fillRect(L.x + 1, ry, L.w - 2, LIST_ROW_H);
    } else if (c.type === "COLORBOX" && i === current) {
      ctx.fillStyle = "rgba(255,187,0,1)";
      ctx.fillRect(L.x + 1, ry, L.w - 2, LIST_ROW_H);
    }
    if (L.acc?.colorAt) {
      // ColorBox rows: colour swatches (row drawing vtbl +0x3c not read, A17).
      ctx.fillStyle = L.acc.colorAt(i);
      ctx.fillRect(L.x + 4, ry + 2, L.w - 8, LIST_ROW_H - 4);
      ctx.strokeStyle = "#000";
      ctx.strokeRect(L.x + 4.5, ry + 2.5, L.w - 9, LIST_ROW_H - 5);
    } else {
      drawText(ctx, fe, font, L.items[i], L.x + 1 + 3, ry + 1, parseColor(hovered ? "0xffffffff" : "0xff000000"));
    }
  }
  ctx.restore();
  if (L.items.length * LIST_ROW_H > L.full) {
    // Scrollbar when clamped to 160 px [VA 0x4db530] (drawn simplified, A17).
    const total = L.items.length * LIST_ROW_H;
    ctx.fillStyle = "rgba(0,0,0,0.35)";
    ctx.fillRect(L.x + L.w - 6, L.y, 5, L.h);
    ctx.fillStyle = "rgba(188,0,0,1)";
    ctx.fillRect(L.x + L.w - 6, L.y + (L.scroll * LIST_ROW_H / total) * L.h, 5, Math.max(8, L.h * L.full / total));
  }
  ctx.strokeStyle = "#000";
  ctx.strokeRect(L.x + 0.5, L.y + 0.5, L.w - 1, L.h - 1);
  if (L.k < 1) fe.host.requestFrame?.();
}

// AccessorWatch: accessor text in Univers8b, white; align 0 left, 1 centred, 2 right
// [data accessor Watch.ctr; VA 0x4bcf90].
function drawAccessorWatch(ctx, c, fe) {
  const [x, y] = attrPair(c.attrs, "pos");
  const font = c.fonts.own ?? c.fonts["Univers8b.ffn"];
  const text = fe.host.accessor?.(c.attrs.accessor)?.text?.() ?? "";
  const w = textWidth(font, text);
  const align = attrInt(c.attrs, "align", 0);
  drawText(ctx, fe, font, text, x - Math.floor((w / 2) * align), y, parseColor(c.attrs.forecolor, 0xffffffff));
}

// CarLogo: shapes 2*index and 2*index+1 of the car's logo pack side by side, centred
// vertically in the control [VA 0x4c1660, 0x4c1b70]; the glint sweep is not drawn (A17).
function drawCarLogo(ctx, c, fe) {
  const logo = fe.host.carLogo?.(c.attrs.accessor);
  const pack = logo && fe.logoPacks?.get(logo.pack.toLowerCase());
  if (!pack) {
    if (logo && !fe.logoPacks?.has(logo.pack.toLowerCase())) fe.loadLogoPack(logo.pack);
    return;
  }
  const [x, y] = attrPair(c.attrs, "pos");
  const [, h] = attrPair(c.attrs, "size", [100, 60]);
  const a = pack[logo.shape];
  const b = pack[logo.shape + 1];
  if (!a) return;
  const total = a.width + (b && b.height > 1 ? b.width : 0);
  const align = attrInt(c.attrs, "align", 0);
  const left = x - (align === 1 ? total / 2 : align === 2 ? total : 0);
  const top = y + Math.floor((h - a.height) / 2);
  drawShape(ctx, a, left, top);
  if (b && b.height > 1) drawShape(ctx, b, left + a.width, top);
}

// ExtraCircle: 8 parts of extracircle.fsh at fixed positions [data extracircle.ctr].
const EXTRA_CIRCLE = [[313, 99], [387, 115], [452, 147], [505, 211], [504, 275], [434, 339], [320, 413], [320, 414]];
function drawExtraCircle(ctx, c) {
  EXTRA_CIRCLE.forEach(([x, y], i) => drawShape(ctx, c.extra?.[i], x, y));
}

// GouraudBox: quad with corner colours cUL/cUR/cBL/cBR [VA 0x4c9f30]; drawn as rows of
// horizontal gradients (corner offsets ignored, A17).
function drawGouraudBox(ctx, c) {
  const a = c.attrs;
  const [x, y] = attrPair(a, "pos");
  const [w, h] = attrPair(a, "size", [50, 50]);
  const ul = parseColor(a.cul, 0), ur = parseColor(a.cur, 0), bl = parseColor(a.cbl, 0), br = parseColor(a.cbr, 0);
  const mix = (p, q, t) => ({ a: p.a + (q.a - p.a) * t, r: p.r + (q.r - p.r) * t, g: p.g + (q.g - p.g) * t, b: p.b + (q.b - p.b) * t });
  for (let row = 0; row < h; row++) {
    const t = h > 1 ? row / (h - 1) : 0;
    const l = mix(ul, bl, t), r = mix(ur, br, t);
    const grad = ctx.createLinearGradient(x, 0, x + w, 0);
    grad.addColorStop(0, cssColor(l));
    grad.addColorStop(1, cssColor(r));
    ctx.fillStyle = grad;
    ctx.fillRect(x, y + row, w, 1);
  }
}

// GlintyLine: the line in its colour; the moving glint is not drawn (A17) [VA 0x51faa0].
function drawGlintyLine(ctx, c) {
  const [x, y] = attrPair(c.attrs, "pos");
  const [w, h] = attrPair(c.attrs, "size", [1, 1]);
  ctx.fillStyle = cssColor(parseColor(c.attrs.color, 0xff000000));
  ctx.fillRect(x, y, Math.max(1, w), Math.max(1, h));
}

function drawHeader(ctx, c, fe) {
  drawText(ctx, fe, c.fonts["Univers17b.ffn"], fe.assets.string(c.attrs.stringid), 6, 4, parseColor("0xffffffff"));
}

function drawLabel(ctx, c, fe) {
  // The cash label belongs to the career account box (FEBottomBarMenu binds ID_CASHBKGD and
  // ID_ACCOUNTTHINGY [VA 0x4d7fd0]); outside the career it is not shown.
  if (c.attrs.name === "ID_ACCOUNTTHINGY" && !fe.host.career?.()) return;
  const [x, y] = attrPair(c.attrs, "pos");
  const font = c.fonts.own;
  const text = fe.assets.string(c.attrs.stringid);
  const align = attrInt(c.attrs, "align", 0);
  const w = textWidth(font, text);
  // Same convention as the text parts: x - width/2 * align (0 left, 1 centred, 2 right).
  const left = x - Math.floor((w / 2) * align);
  drawText(ctx, fe, font, text, left, y, parseColor(c.attrs.forecolor, 0xffffffff));
}

function drawRectangle(ctx, c) {
  const [x, y] = attrPair(c.attrs, "pos");
  const [w, h] = attrPair(c.attrs, "size", [50, 50]);
  ctx.fillStyle = cssColor(parseColor(c.attrs.color, 0x8000ff00));
  ctx.fillRect(x, y, w, h);
}

// BOTTOMBARBUTTON [data BottomBarButton.ctr]: animated icon at (0, picoffy), label in
// Univers11b at (icon width + space, 8); align=1 makes posx the right edge.
function bottomBarRect(c, fe) {
  const a = c.attrs;
  const icon = c.shapes?.[attrInt(a, "startframe", 0)];
  const font = c.fonts["Univers11b.ffn"];
  const text = fe.assets.string(a.stringid);
  const iconW = icon?.width ?? 0;
  const w = iconW + (text ? attrInt(a, "space", 0) + textWidth(font, text) : 0);
  const h = Math.max((icon?.height ?? 0) + attrInt(a, "picoffy", 0), 20);
  let x = attrInt(a, "posx", 0);
  if (attrInt(a, "align", 0) === 1) x -= w;
  return { x, y: attrInt(a, "posy", 0), w, h, iconW, text, font };
}

function animationFrame(start, end, ticks, ms) {
  if (end <= start || ticks <= 0) return start;
  const span = end - start + 1;
  return start + Math.floor((ms * 128 / 1000) / (ticks / span)) % span;
}

function drawBottomBarButton(ctx, c, fe, now) {
  const a = c.attrs;
  const r = bottomBarRect(c, fe);
  const disabled = fe.disabled(c, fe.bottombar);
  const hover = !disabled && fe.hover === c;
  const frame = hover
    ? animationFrame(attrInt(a, "hoverstartframe", 0), attrInt(a, "hoverendframe", 0), attrInt(a, "hovertotalticks", 0), now - (fe.hoverStarted ?? now))
    : animationFrame(attrInt(a, "startframe", 0), attrInt(a, "endframe", 0), attrInt(a, "totalticks", 0), now);
  ctx.save();
  if (disabled) ctx.globalAlpha *= 0.5; // disabled look of the icon not read (A17)
  drawShape(ctx, c.shapes?.[frame], r.x, r.y + attrInt(a, "picoffy", 0), attrInt(a, "flip", 0));
  ctx.restore();
  if (r.text) {
    const color = parseColor(disabled ? "0xff808080" : hover ? "0xffffbb00" : "0xffffffff");
    drawText(ctx, fe, r.font, r.text, r.x + r.iconW + attrInt(a, "space", 0), r.y + 8, color);
  }
  if (hover || frame !== attrInt(a, "startframe", 0) || attrInt(a, "totalticks", 0) > 0) fe.host.requestFrame?.();
}

// MOVIECIRCLE without movie playback: only the grid overlay inside the circle (A17); the
// porthole shows the room [VA 0x4cdae0, 0x4cdf40].
function drawMovieCircle(ctx, c) {
  const [x, y] = attrPair(c.attrs, "pos");
  const [w, h] = attrPair(c.attrs, "size", [416, 348]);
  const cx = x + w / 2, cy = y + h / 2 + 4, rx = 207, ry = 180;
  ctx.save();
  ctx.beginPath();
  ctx.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2);
  ctx.clip();
  ctx.fillStyle = "rgba(255,255,255,0.3)";
  for (let i = 1; i <= 3; i++) {
    ctx.fillRect(cx - rx + (2 * rx * i) / 4, cy - ry, 1, 2 * ry);
    ctx.fillRect(cx - rx, cy - ry + (2 * ry * i) / 4, 2 * rx, 1);
  }
  ctx.restore();
}

const CONTROL_DRAW = {
  PICTURE: drawPicture,
  GOTOBUTTON: drawGotoButton,
  COMBOBOXSLIDER: drawComboBox,
  COLORBOX: drawComboBox,
  ACCESSORWATCH: drawAccessorWatch,
  CARLOGO: drawCarLogo,
  EXTRACIRCLE: drawExtraCircle,
  GOURAUDBOX: drawGouraudBox,
  GLINTYLINE: drawGlintyLine,
  HEADER: drawHeader,
  LABEL: drawLabel,
  RECTANGLE: drawRectangle,
  BOTTOMBARBUTTON: drawBottomBarButton,
  MOVIECIRCLE: drawMovieCircle,
};

const CONTROL_FONTS = {
  GOTOBUTTON: ["UniversC11b.ffn", "UniversC11m.ffn", "Univers8b.ffn"],
  COMBOBOXSLIDER: ["UniversC11b.ffn", "UniversC11m.ffn", "Univers8b.ffn"],
  COLORBOX: ["UniversC11b.ffn", "UniversC11m.ffn", "Univers8b.ffn"],
  ACCESSORWATCH: ["Univers8b.ffn"],
  HEADER: ["Univers17b.ffn"],
  BOTTOMBARBUTTON: ["Univers11b.ffn"],
};

const CONTROL_HIT = {
  GOTOBUTTON: c => {
    const [x, y] = attrPair(c.attrs, "pos");
    return { x, y, w: attrInt(c.attrs, "width", 157), h: attrInt(c.attrs, "height", 18) };
  },
  COMBOBOXSLIDER: (c, fe) => {
    const g = comboGeometry(c, fe);
    return { x: g.x, y: g.y, w: g.w, h: g.h };
  },
  COLORBOX: (c, fe) => CONTROL_HIT.COMBOBOXSLIDER(c, fe),
  BOTTOMBARBUTTON: (c, fe) => bottomBarRect(c, fe),
};

export const SUPPORTED_CONTROLS = Object.keys(CONTROL_DRAW);
export { FE_WIDTH, FE_HEIGHT };

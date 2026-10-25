// Original front end: screens built from FEData layouts, drawn on a 640x480 canvas over the
// 3D view (the room). Evidence and control semantics: docs/re/frontend.md (workspace root).
import {
  FE_WIDTH, FE_HEIGHT, backdropLayoutName, attrInt, attrPair, parseColor, cssColor,
  fitViewport, toLayoutPoint, inRect, transitionState, nextStack,
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
    this.stack = [];
    this.screen = null;
    this.hover = null;
    this.pressed = null;
    this.visible = false;
    this.view = fitViewport(1, 1);
    canvas.addEventListener("pointermove", e => this.pointer("move", e));
    canvas.addEventListener("pointerdown", e => this.pointer("down", e));
    canvas.addEventListener("pointerup", e => this.pointer("up", e));
    canvas.addEventListener("pointerleave", () => { this.hover = null; });
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
      c.fonts = {};
      for (const font of CONTROL_FONTS[c.type] ?? []) c.fonts[font] = await this.assets.font(font);
      if (a.fontpathname) c.fonts.own = await this.assets.font(a.fontpathname);
    }
    return { name, screenAttrs, controls };
  }

  async show(name) {
    const layout = await this.prepare(name);
    if (!layout) return false;
    const backdropName = backdropLayoutName(layout.screenAttrs.screen || "ID_SCREEN");
    layout.backdrop = backdropName ? await this.prepare(backdropName) : null;
    layout.started = performance.now();
    this.screen = layout;
    this.loading = null;
    this.hover = null;
    this.visible = true;
    const car = layout.controls.find(c => c.type === "3DCAR");
    await this.host.onRoom?.(layout.screenAttrs.room || "ID_TESTROOM", car ? (car.attrs.accessor || "zMergedList.Car") : null);
    this.host.requestFrame?.();
    return true;
  }

  async goto(name, offset = 0) {
    this.stack = nextStack(this.stack, name.toLowerCase(), offset);
    return this.show(this.stack[this.stack.length - 1]);
  }

  async back() {
    if (this.stack.length <= 1) return false;
    this.stack = this.stack.slice(0, -1);
    return this.show(this.stack[this.stack.length - 1]);
  }

  async start(name = "gamesetup") {
    this.stack = [];
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
    for (const layout of [this.screen.backdrop, this.screen, this.bottombar]) {
      if (!layout) continue;
      for (let i = layout.controls.length - 1; i >= 0; i--) out.push({ layout, control: layout.controls[i] });
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
      const fromScreen = layout === this.screen;
      const t = transitionState(fromScreen ? control.attrs.transon : "FADE", elapsed);
      ctx.save();
      ctx.globalAlpha = t.alpha;
      ctx.translate(t.dx, t.dy);
      draw(ctx, control, this, now);
      ctx.restore();
    }
    if (elapsed < 600) this.host.requestFrame?.();
  }

  // Interactive controls in hit-test order (front first).
  hitTest(x, y) {
    const layers = this.layers();
    for (let i = layers.length - 1; i >= 0; i--) {
      const { control, layout } = layers[i];
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
    const hit = this.hitTest(x, y);
    if (type === "move") {
      if (hit !== this.hover) { this.hover = hit; this.hoverStarted = performance.now(); }
    } else if (type === "down") {
      this.pressed = hit;
    } else if (type === "up") {
      if (hit && hit === this.pressed) this.activate(hit);
      this.pressed = null;
    }
    this.canvas.style.cursor = hit ? "pointer" : "default";
    this.host.requestFrame?.();
  }

  // GOTOBUTTON: callback first, then destination with its stack offset [VA 0x4c9470].
  async activate(control) {
    const a = control.attrs;
    const callback = (a.callback || "").toUpperCase();
    if (callback === "GOBACK") return this.back();
    if (callback && this.host.onAction?.(callback, control, this) === true) return;
    if (control.type === "COMBOBOXSLIDER" && this.host.onAction?.("COMBOBOX", control, this) === true) return;
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

function textWidth(font, text) {
  if (!font) return 0;
  let w = 0;
  for (const ch of text) w += (font.glyphs.get(ch.charCodeAt(0)) ?? font.glyphs.get(32))?.advance ?? 0;
  return w;
}

function drawText(ctx, fe, font, text, x, y, color) {
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

// ComboBoxSlider: drawn as a GOTOBUTTON bar with the label left and the accessor value right
// (class code not read yet; A17).
function drawComboBox(ctx, c, fe) {
  const r = drawGotoButton(ctx, c, fe);
  const font = c.fonts["UniversC11b.ffn"];
  const value = `${fe.host.accessor?.(c.attrs.accessor) ?? ""} ▼`;
  drawText(ctx, fe, font, value, r.x + r.w - 6 - textWidth(font, value), r.y + 2, parseColor("0xffffffff"));
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
  const left = align === 1 ? x - w : align === 2 ? x - Math.floor(w / 2) : x;
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
  HEADER: drawHeader,
  LABEL: drawLabel,
  RECTANGLE: drawRectangle,
  BOTTOMBARBUTTON: drawBottomBarButton,
  MOVIECIRCLE: drawMovieCircle,
};

const CONTROL_FONTS = {
  GOTOBUTTON: ["UniversC11b.ffn", "UniversC11m.ffn", "Univers8b.ffn"],
  COMBOBOXSLIDER: ["UniversC11b.ffn"],
  HEADER: ["Univers17b.ffn"],
  BOTTOMBARBUTTON: ["Univers11b.ffn"],
};

const CONTROL_HIT = {
  GOTOBUTTON: c => {
    const [x, y] = attrPair(c.attrs, "pos");
    return { x, y, w: attrInt(c.attrs, "width", 157), h: attrInt(c.attrs, "height", 18) };
  },
  COMBOBOXSLIDER: c => CONTROL_HIT.GOTOBUTTON(c),
  BOTTOMBARBUTTON: (c, fe) => bottomBarRect(c, fe),
};

export const SUPPORTED_CONTROLS = Object.keys(CONTROL_DRAW);
export { FE_WIDTH, FE_HEIGHT };

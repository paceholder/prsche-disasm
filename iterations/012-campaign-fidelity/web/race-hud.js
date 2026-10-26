// Original race HUD (docs/re/hud.md): draws the 2D elements on #hudCanvas, 1 HUD unit = 1 CSS px.
// Logic lives in race-hud-core.js; shortcuts are registered in docs/approximations.md.
import {
  parseHudFiles, selectConfig, SLOT, carGauges, gaugePacks, NEEDLE_SIZES, speedoUnits, tachUnits,
  needleAngle, dialCentre, accentColor, gearText, formatTicks, secondsToTicks,
  BOX_ALPHA_TOP, BOX_ALPHA_BOTTOM, damageDraws, damageColor, mapZoom, markerScale, carHudColour,
  parseCourseMap, mapTransform, countdownDraws, COUNTDOWN_LAST_STEP,
} from "./race-hud-core.js";
import { FeAssets, makeCanvas } from "./fe-assets.js";
import { drawText, textWidth } from "./frontend.js";

const DASH = "GameData/DashHud";
const CAR_RECORD = 0x670;
const PHASE_RACING = 2;

const argb = n => ({ a: (n >>> 24) & 255, r: (n >>> 16) & 255, g: (n >>> 8) & 255, b: n & 255 });
const rgbaCss = n => { const c = argb(n); return `rgba(${c.r},${c.g},${c.b},${(c.a / 255).toFixed(3)})`; };
const rgbCss = n => { const c = argb(n); return `rgb(${c.r},${c.g},${c.b})`; };

export class RaceHud {
  // opts: { canvas: #hudCanvas, view: element whose CSS size is the 3D view, api, readFile }
  constructor({ canvas, view, api, readFile }) {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d");
    this.view = view;
    this.readFile = readFile;
    this.assets = new FeAssets(api, readFile);
    this.fe = { assets: this.assets };
    this.records = [];
    this.config = null;
    this.size = [0, 0];
    this.gauges = null;
    this.packs = {};
    this.fonts = {};
    this.strings = {};
    this.ready = false;
    this.raceTime = 0;
    this.lastPhase = 0;
    this.tinted = new Map();
    this.outline = { segs: null, at: -1e9, closed: false, bbox: null };
    this.standings = { at: -1e9, rows: [] };
    this.last = { speedUnits: 0, tachUnits: 0, gear: "" };
    this.nightPacks = null;
  }

  async init() {
    const [h1, h2] = await Promise.all([
      this.readFile("FEData/Data/nfs5-1.hud"), this.readFile("FEData/Data/nfs5-2.hud")]);
    if (!h1 || !h2) throw new Error("HUD config files missing");
    this.records = parseHudFiles(h1, h2);
    await this.assets.loadStrings(0, `${DASH}/hud.loc`);
    this.strings = this.assets.strings;
    for (const [key, file] of [["big", "UniversC12b.ffn"], ["list", "unviers9b.ffn"], ["listi", "unviers9bi.ffn"]]) {
      this.fonts[key] = await this.assets.font(file, DASH);
    }
    const names = ["circletach", "speedK", "speedKN", "speedM", "speedMN", "needle", "needleN",
      "needle2", "needle2N", "tach", "tachN", "car", "radar", "321go", "321go2", "circlefull", "circleoutline"];
    await Promise.all(names.map(async n => { this.packs[n] = await this.assets.pack(`${n}.fsh`, DASH); }));
    this.ready = true;
  }

  // Gauge settings from the nfs5.car record of the race car.
  setCarRecord(bytes, index) {
    if (!bytes || index == null || index < 0 || (index + 1) * CAR_RECORD > bytes.length) { this.gauges = null; return; }
    this.gauges = carGauges(bytes.subarray(index * CAR_RECORD, (index + 1) * CAR_RECORD), 0);
  }

  str(id, fallback) { return this.strings[String(id)] ?? fallback; }

  clear() {
    const { ctx, canvas } = this;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
  }

  syncSize() {
    const rect = this.view.getBoundingClientRect();
    const W = Math.max(1, Math.round(rect.width));
    const H = Math.max(1, Math.round(rect.height));
    const dpr = window.devicePixelRatio || 1;
    const pw = Math.round(W * dpr);
    const ph = Math.round(H * dpr);
    if (this.canvas.width !== pw || this.canvas.height !== ph) {
      this.canvas.width = pw;
      this.canvas.height = ph;
    }
    if (W !== this.size[0] || H !== this.size[1] || !this.config) {
      this.size = [W, H];
      this.config = selectConfig(this.records, 0, W, H);
    }
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  // Colour mask of a shape: its alpha with an RGB fill (cached).
  tint(shape, color) {
    const key = `${shape.name}|${shape.width}x${shape.height}|${color & 0xffffff}`;
    let c = this.tinted.get(key);
    if (!c) {
      c = makeCanvas(shape.width, shape.height);
      const g = c.getContext("2d");
      g.drawImage(shape.canvas, 0, 0);
      g.globalCompositeOperation = "source-in";
      g.fillStyle = rgbCss(color);
      g.fillRect(0, 0, shape.width, shape.height);
      this.tinted.set(key, c);
    }
    return c;
  }

  // Per-frame entry. `viewer`: wasm viewer; `dt` seconds; `lights` day/night packs.
  draw(viewer, { dt = 0, lights = false, carColours = null } = {}) {
    this.carColours = carColours;
    if (!this.ready || !this.gauges) { this.clear(); return; }
    this.syncSize();
    const cfg = this.config;
    if (!cfg) return;
    const { ctx } = this;
    const [W, H] = this.size;
    ctx.clearRect(0, 0, W, H);
    const tel = viewer.get_hud_telemetry();
    if (!tel || tel.length < 13) return;
    // Test hook: `__nfs.hud.debugDamage = [z0..z8]` replaces the car's zone damage in the panel.
    if (this.debugDamage) for (let i = 0; i < 9; i++) tel[4 + i] = this.debugDamage[i] ?? 0;
    const phase = viewer.get_race_phase();
    if (phase === PHASE_RACING) this.raceTime += dt;
    else if (phase < PHASE_RACING) this.raceTime = 0;
    this.lastPhase = phase;
    const o = cfg.options;
    const S = cfg.slots;
    const accent = accentColor(o.displayColors);
    const grey = cfg.colors[0];
    const listGrey = cfg.colors[1];
    const padX = cfg.hud1.getInt32(0x34, true) || 5;
    const padY = cfg.hud1.getInt32(0x38, true) || 2;
    const box = cfg.translucency !== 0;
    const st = { accent, grey, listGrey, padX, padY, box, tel, viewer, lights, phase };

    const rows = this.getStandings(viewer);
    const listBox = { x: S[4].x, y: S[4].y, w: S[4].w, h: (rows.length + 1) * S[4].h + 2 * padY };
    if (box) {
      if (o.gear === 1) this.panel(S[0]);
      this.panel(S[2]);
      if (o.opponentList !== 0 && rows.length) this.panel(listBox);
      if (o.damageDisplay === 1) this.panel(S[5]);
      if (o.splitTimes !== 0) this.panel(S[11]);
    }

    if (o.speedometer === 2) this.dial(S[SLOT.SPEEDO], "speedo", st);
    if (o.tachometer === 2) this.dial(S[SLOT.TACH], "tach", st);
    this.last.gear = gearText(tel[2], this.strings);
    if (o.gear === 1) this.combo(S[0], this.last.gear, this.str(10, "gear"), st);
    this.combo(S[2], formatTicks(secondsToTicks(this.raceTime)), this.str(18, "time"), st);
    if (o.splitTimes !== 0) {
      const label = o.splitTimes === 1 ? this.str(22, "record lap") : o.splitTimes === 2 ? this.str(23, "best lap") : this.str(24, "last lap");
      const best = viewer.get_best_lap_time();
      this.combo(S[11], formatTicks(best > 0 && Number.isFinite(best) ? secondsToTicks(best) : 0), label, st);
    }
    if (o.opponentList !== 0 && rows.length) this.positionList(listBox, S[4], rows, st);
    if (o.damageDisplay === 1) this.damage(S[5], st);
    if (o.map !== 0) this.map(S[SLOT.MAP], st, o);
    this.countdown(viewer, phase, W, H);
  }

  // Start countdown H321Go (0x4184f0), visible from step 0x100 to 0x1c0 of the 7 s start.
  countdown(viewer, phase, W, H) {
    this.last.countdown = null;
    let step = this.debugCountdownStep ?? null; // test hook: __nfs.hud.debugCountdownStep = 0x100..0x1c0
    if (step === null && phase === 1) step = Math.floor((COUNTDOWN_LAST_STEP / 64 - viewer.get_countdown_remaining()) * 64);
    if (step === null) return;
    const sizes = (pack, frame) => {
      const sh = this.packs[pack]?.[frame];
      return [sh?.width ?? 0, sh?.height ?? 0];
    };
    const draws = countdownDraws(step, W, H, sizes);
    if (!draws) return;
    this.last.countdown = { step, items: draws.length };
    const { ctx } = this;
    for (const d of draws) {
      const a = (d.color >>> 24) / 255;
      if (d.kind === "sprite") {
        const sh = this.packs[d.art[0]]?.[d.art[1]];
        if (!sh?.canvas || a <= 0) continue;
        const tinted = (d.color & 0xffffff) === 0xffffff ? sh.canvas : this.tint(sh, d.color & 0xffffff);
        ctx.globalAlpha = a;
        ctx.drawImage(tinted, d.x, d.y, d.w, d.h);
        ctx.globalAlpha = 1;
      } else if (d.kind === "line") {
        ctx.strokeStyle = rgbaCss(d.color);
        ctx.lineWidth = 1;
        ctx.beginPath();
        const h = d.y0 === d.y1;
        ctx.moveTo(h ? d.x0 : d.x0 + 0.5, h ? d.y0 + 0.5 : d.y0);
        ctx.lineTo(h ? d.x1 : d.x1 + 0.5, h ? d.y1 + 0.5 : d.y1);
        ctx.stroke();
      } else if (d.kind === "wedge") {
        ctx.fillStyle = rgbaCss(d.color);
        ctx.beginPath();
        for (const [p0, p1, p2] of d.tris) {
          ctx.moveTo(p0[0], p0[1]);
          ctx.lineTo(p1[0], p1[1]);
          ctx.lineTo(p2[0], p2[1]);
          ctx.closePath();
        }
        ctx.fill();
      }
    }
  }

  panel(b) {
    const { ctx } = this;
    const g = ctx.createLinearGradient(0, b.y, 0, b.y + b.h);
    g.addColorStop(0, `rgba(0,0,0,${BOX_ALPHA_TOP / 255})`);
    g.addColorStop(1, `rgba(0,0,0,${BOX_ALPHA_BOTTOM / 255})`);
    ctx.fillStyle = g;
    ctx.fillRect(b.x, b.y, b.w, b.h);
  }

  text(font, str, x, y, color) { drawText(this.ctx, this.fe, font, str, x, y, argb(color)); }

  // Value top-right in the accent colour, label bottom-left in grey [VA 0x41c740..0x41ca10].
  combo(b, value, label, st) {
    const f = this.fonts.big;
    if (!f) return;
    this.text(f, value, b.x + b.w - st.padX - textWidth(f, value), b.y + st.padY, st.accent);
    this.text(f, label, b.x + st.padX, b.y + b.h - st.padY - f.lineHeight, st.grey);
  }

  dial(b, kind, st) {
    const g = this.gauges;
    const gauge = kind === "speedo" ? g.speedo : g.tach;
    const night = st.lights;
    const names = gaugePacks(night, 0);
    const face = (this.packs[kind === "speedo" ? names.speedFace : names.tachFace] ?? this.packs[kind === "speedo" ? "speedK" : "tach"])?.[gauge.style];
    const needlePack = this.packs[kind === "speedo" ? names.speedNeedle : names.tachNeedle] ?? this.packs[kind === "speedo" ? "needle2" : "needle"];
    const backing = this.packs.circletach?.[0];
    const { ctx } = this;
    if (st.box && backing?.canvas) {
      ctx.globalAlpha = this.config.translucency / 255;
      ctx.drawImage(backing.canvas, b.x, b.y);
      ctx.globalAlpha = 1;
    }
    if (face?.canvas) ctx.drawImage(face.canvas, b.x, b.y);
    const value = kind === "speedo" ? speedoUnits(st.tel[0], 0) : tachUnits(st.tel[1], st.tel[3]);
    this.last[kind === "speedo" ? "speedUnits" : "tachUnits"] = value;
    const [cx, cy] = dialCentre(b);
    const size = NEEDLE_SIZES[g.needleStyle] ?? NEEDLE_SIZES[0];
    const needle = needlePack?.[g.needleStyle];
    if (needle?.canvas) {
      const angle = needleAngle(value, gauge);
      const [w, len] = size;
      const half = Math.trunc((w + 1) / 2);
      ctx.save();
      ctx.translate(cx, cy);
      ctx.rotate(Math.PI / 2 - angle);
      ctx.drawImage(needle.canvas, -half, -(len - g.needlePivot), 2 * half, len);
      ctx.restore();
    }
    if (g.needleStyle === 0) {
      const cap = needlePack?.[5];
      if (cap?.canvas) ctx.drawImage(cap.canvas, cx - Math.trunc(cap.width / 2), cy - Math.trunc(cap.height / 2));
    }
  }

  getStandings(viewer) {
    const now = performance.now();
    if (now - this.standings.at > 150) {
      let rows = [];
      try { rows = JSON.parse(viewer.get_race_standings_json()); } catch { rows = []; }
      rows.forEach((r, i) => { r.slot = i; });
      rows.sort((a, b) => a.rank - b.rank);
      this.standings = { at: now, rows };
    }
    return this.standings.rows;
  }

  positionList(listBox, row, rows, st) {
    const small = this.fonts.list;
    const smallI = this.fonts.listi;
    const big = this.fonts.big;
    const sq = [this.config.hud1.getInt32(0x114, true), this.config.hud1.getInt32(0x118, true)];
    const sw = sq[0] > 0 && sq[0] < 40 ? sq[0] : 8;
    const sh = sq[1] > 0 && sq[1] < 40 ? sq[1] : 8;
    const player = rows.find(r => r.player) ?? rows[0];
    const { ctx } = this;
    rows.forEach((r, i) => {
      const y = row.y + st.padY + row.h * i;
      ctx.fillStyle = rgbCss(carHudColour(this.carColours, r.slot));
      ctx.fillRect(row.x + st.padX, y + Math.trunc((row.h - sh) / 2), sw, sh);
      const ty = y + Math.trunc((row.h - (small?.lineHeight ?? 12)) / 2);
      this.text(small, r.name, row.x + st.padX + sw + 4, ty, st.listGrey);
      let gap = "-----";
      if (!r.player) {
        const d = Math.trunc(r.progress - player.progress);
        gap = String(d);
      }
      this.text(smallI, gap, row.x + row.w - st.padX - 8 - textWidth(smallI, gap), ty, st.accent);
    });
    const y = row.y + st.padY + row.h * rows.length;
    this.text(big, this.str(17, "position"), row.x + st.padX, y + Math.trunc((row.h - big.lineHeight) / 2), st.grey);
    const total = `/${rows.length}`;
    const rank = String(player.rank);
    let x = row.x + row.w - st.padX - textWidth(small, total);
    this.text(small, total, x, y + Math.trunc((row.h - small.lineHeight) / 2) + 2, st.listGrey);
    x -= textWidth(big, rank);
    this.text(big, rank, x, y + Math.trunc((row.h - big.lineHeight) / 2), st.accent);
  }

  damage(b, st) {
    const { ctx } = this;
    const pack = this.packs.car;
    const ox = b.x + this.config.hud1.getInt32(0x104, true);
    const oy = b.y + this.config.hud1.getInt32(0x108, true);
    for (const d of damageDraws()) {
      const shape = pack?.[d.shape];
      if (!shape?.canvas) continue;
      const w = d.w ?? shape.width;
      const h = d.h ?? shape.height;
      let img = shape.canvas;
      if (d.zone >= 0) {
        const color = damageColor(st.tel[4 + d.zone]);
        ctx.globalAlpha = ((color >>> 24) & 255) / 255;
        img = this.tint(shape, color);
      }
      if (d.flip) {
        ctx.save();
        ctx.translate(ox + d.x, oy + d.y + h);
        ctx.scale(1, -1);
        ctx.drawImage(img, 0, 0, w, h);
        ctx.restore();
      } else {
        ctx.drawImage(img, ox + d.x, oy + d.y, w, h);
      }
      ctx.globalAlpha = 1;
    }
    const bx = b.x + 5;
    const bw = b.w - 10;
    const by = b.y + 72;
    const grad = ctx.createLinearGradient(bx, 0, bx + bw, 0);
    grad.addColorStop(0, rgbaCss(0xb2058228));
    grad.addColorStop(0.5, rgbaCss(0xb2ffff00));
    grad.addColorStop(1, rgbaCss(0xb2dc1905));
    ctx.fillStyle = grad;
    ctx.fillRect(bx, by, bw, 6);
    const li = this.fonts.listi;
    this.text(li, "0%", bx, by - li.lineHeight - 1, st.accent);
    this.text(li, "100%", bx + bw - textWidth(li, "100%"), by - li.lineHeight - 1, st.accent);
    const big = this.fonts.big;
    this.text(big, this.str(12, "damage"), b.x + st.padX, b.y + b.h - st.padY - big.lineHeight, st.grey);
  }

  getOutline(viewer) {
    const now = performance.now();
    if (now - this.outline.at > 2000 || !this.outline.segs) {
      const raw = Array.from(viewer.get_course_map());
      this.outline.at = now;
      const sig = raw.length ? `${raw.length}|${raw[0]}|${raw[1]}|${raw[2]}|${raw[raw.length - 2]}` : "";
      if (sig !== this.outline.sig) {
        this.outline = { ...parseCourseMap(raw), sig, at: now };
      }
    }
    return this.outline;
  }

  map(b, st, o) {
    const { ctx } = this;
    const viewer = st.viewer;
    const markers = Array.from(viewer.get_car_markers());
    if (markers.length < 8) return;
    const outline = this.getOutline(viewer);
    ctx.save();
    ctx.beginPath();
    ctx.rect(b.x, b.y, b.w, b.h);
    ctx.clip();
    if (st.box) {
      ctx.fillStyle = "rgba(0,0,0,0.247)";
      ctx.fillRect(b.x, b.y, b.w, b.h);
    }
    const spacing = this.config.hud1.getInt32(0x10c, true) || 20;
    ctx.strokeStyle = rgbaCss(this.config.colors[6]);
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let x = spacing; x < b.w; x += spacing) { ctx.moveTo(b.x + x + 0.5, b.y); ctx.lineTo(b.x + x + 0.5, b.y + b.h); }
    for (let y = spacing; y < b.h; y += spacing) { ctx.moveTo(b.x, b.y + y + 0.5); ctx.lineTo(b.x + b.w, b.y + y + 0.5); }
    ctx.stroke();

    const zoom = mapZoom(o.map, o.mapScale);
    const px = markers[0], pz = markers[1];
    const yaw = Math.atan2(markers[2], markers[3]);
    let cx = px, cz = pz, Z = zoom.z, up = o.map === 1 ? yaw : 0;
    if (zoom.whole && outline.bbox) { cx = outline.bbox.cx; cz = outline.bbox.cz; Z = outline.bbox.ext / 2 + 200; up = 0; }
    // The world is right-handed (right = (-fz, fx)); the unmirrored transform would put it on the left.
    const mirror = true;
    const tf = mapTransform(b, cx, cz, Z, up, mirror);
    const scale = b.w / 2 / Z;
    const segs = outline.segs;
    if (segs && segs.length >= 4) {
      ctx.strokeStyle = "#fff";
      ctx.beginPath();
      for (let i = 0; i + 3 < segs.length; i += 4) {
        const [x0, y0] = tf(segs[i], segs[i + 1]);
        const [x1, y1] = tf(segs[i + 2], segs[i + 3]);
        ctx.moveTo(x0, y0);
        ctx.lineTo(x1, y1);
      }
      ctx.stroke();
    }
    const car = this.packs.radar?.[0];
    const k = markerScale(o.map, zoom.whole);
    for (let i = Math.floor(markers.length / 8) - 1; i >= 0; i--) {
      const m = markers.slice(i * 8, i * 8 + 8);
      const [x, y] = tf(m[0], m[1]);
      const rel = Math.atan2(m[2], m[3]) - up;
      const w = Math.max(2, 2 * m[4] * k * scale);
      const l = Math.max(3, 2 * m[5] * k * scale);
      if (!car?.canvas) continue;
      const color = carHudColour(this.carColours, i);
      ctx.save();
      ctx.translate(x, y);
      ctx.rotate(mirror ? -rel : rel);
      ctx.drawImage(this.tint(car, color), -w / 2, -l / 2, w, l);
      ctx.restore();
    }
    if (outline.closed) {
      const f = this.fonts.list;
      const lap = `${this.str(19, "lap")} ${viewer.get_current_lap()}/${viewer.get_total_laps()}`;
      this.text(f, lap, b.x + st.padX, b.y + b.h - st.padY - f.lineHeight, st.accent);
    }
    ctx.restore();
  }
}

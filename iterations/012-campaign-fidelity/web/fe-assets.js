// Original front-end assets in the browser: layouts, FSH shapes, FFN fonts and strings,
// decoded by the wasm module (fe_parse_layout, fe_decode_fsh, fe_parse_font, fe_parse_strings).
import { parseLayoutJson } from "./fe-core.js";

export class FeAssets {
  // api: wasm functions; readFile(path) -> Promise<Uint8Array|null>, path relative to the game dir.
  constructor(api, readFile) {
    this.api = api;
    this.readFile = readFile;
    this.layouts = new Map();
    this.packs = new Map();
    this.fonts = new Map();
    this.tinted = new Map();
    this.strings = {};
  }

  async loadStrings(language = 0) {
    const bytes = await this.readFile("FEData/Locale/festrings.loc");
    if (bytes) this.strings = JSON.parse(this.api.fe_parse_strings(bytes, language));
  }

  string(id) {
    // stringID=0 means "no text" (string 0 is the "Hello, World!" test entry).
    const n = parseInt(id, 10);
    if (!Number.isFinite(n) || n <= 0) return "";
    return this.strings[String(n)] ?? "";
  }

  async layout(name) {
    const key = name.toLowerCase().replace(/\.lay$/, "");
    if (!this.layouts.has(key)) {
      this.layouts.set(key, (async () => {
        const bytes = await this.readFile(`FEData/Layouts/${key}.lay`);
        return bytes ? parseLayoutJson(this.api.fe_parse_layout(bytes)) : null;
      })());
    }
    return this.layouts.get(key);
  }

  // Shapes of FEData/Art/<file> as canvases (null for undecodable shapes).
  async pack(file, dir = "FEData/Art") {
    const key = `${dir}/${file}`.toLowerCase();
    if (!this.packs.has(key)) {
      this.packs.set(key, (async () => {
        const bytes = await this.readFile(`${dir}/${file}`);
        if (!bytes) return null;
        return this.api.fe_decode_fsh(bytes).map(shape => ({
          name: shape.name, width: shape.width, height: shape.height,
          x: shape.x ?? 0, y: shape.y ?? 0, cx: shape.cx ?? 0, cy: shape.cy ?? 0,
          canvas: shape.rgba ? toCanvas(shape.width, shape.height, shape.rgba) : null,
        }));
      })());
    }
    return this.packs.get(key);
  }

  async font(file) {
    const key = file.toLowerCase();
    if (!this.fonts.has(key)) {
      this.fonts.set(key, (async () => {
        const bytes = await this.readFile(`FEData/Art/${file}`);
        if (!bytes) return null;
        const f = this.api.fe_parse_font(bytes);
        const rgba = new Uint8ClampedArray(f.width * f.height * 4);
        for (let i = 0; i < f.width * f.height; i++) {
          rgba[i * 4] = rgba[i * 4 + 1] = rgba[i * 4 + 2] = 255;
          rgba[i * 4 + 3] = f.alpha[i];
        }
        const glyphs = new Map(f.glyphs.map(([code, w, h, x, y, advance, xo, yo]) =>
          [code, { w, h, x, y, advance, xo, yo }]));
        return { key, lineHeight: f.lineHeight, glyphs, atlas: toCanvas(f.width, f.height, rgba) };
      })());
    }
    return this.fonts.get(key);
  }

  // Font atlas multiplied by a CSS colour (cached per font and colour).
  tintedAtlas(font, color) {
    const key = `${font.key}|${color}`;
    let atlas = this.tinted.get(key);
    if (!atlas) {
      atlas = makeCanvas(font.atlas.width, font.atlas.height);
      const ctx = atlas.getContext("2d");
      ctx.drawImage(font.atlas, 0, 0);
      ctx.globalCompositeOperation = "source-in";
      ctx.fillStyle = color;
      ctx.fillRect(0, 0, atlas.width, atlas.height);
      this.tinted.set(key, atlas);
    }
    return atlas;
  }
}

export function makeCanvas(width, height) {
  if (typeof OffscreenCanvas !== "undefined") return new OffscreenCanvas(width, height);
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  return canvas;
}

function toCanvas(width, height, rgba) {
  const canvas = makeCanvas(width, height);
  canvas.getContext("2d").putImageData(new ImageData(new Uint8ClampedArray(rgba), width, height), 0, 0);
  return canvas;
}

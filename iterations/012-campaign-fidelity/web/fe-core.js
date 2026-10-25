// Original front end: pure layout logic (no DOM), shared by frontend.js and frontend.test.mjs.
// Semantics: docs/re/frontend.md and docs/formats/lay.md in the workspace root.

export const FE_WIDTH = 640;
export const FE_HEIGHT = 480;

// `screen=` of section 0 -> backdrop layout drawn under the screen [VA 0x4e1470].
// ID_OPTIONOVAL picks screenoptionovallite in some in-game cases; the menus use the full one.
export const SCREEN_LAYOUTS = {
  ID_SCREEN: "screenempty",
  ID_DARKOVAL: "screendarkoval",
  ID_FULLOVAL: "screenfulloval",
  ID_MOVIEOVAL: "screenmovieoval",
  ID_OPTIONOVAL: "screenoptionoval",
};
export const FALLBACK_SCREEN_LAYOUT = "screenveryempty";

export function backdropLayoutName(screenId) {
  if (!screenId) return null;
  return SCREEN_LAYOUTS[screenId.toUpperCase()] ?? FALLBACK_SCREEN_LAYOUT;
}

// `fe_parse_layout` JSON -> [{index, type, attrs}] in file order.
export function parseLayoutJson(json) {
  const sections = typeof json === "string" ? JSON.parse(json) : json;
  return sections.map(({ index, attrs }) => {
    const id = attrs.id ?? "";
    const dot = id.indexOf(".");
    return { index, type: (dot >= 0 ? id.slice(dot + 1) : id).toUpperCase(), attrs };
  });
}

export function attrInt(attrs, key, fallback = 0) {
  const text = attrs?.[key.toLowerCase()];
  if (text === undefined || text === "") return fallback;
  const value = /^-?0x/i.test(text.trim()) ? parseInt(text.trim().replace(/^(-?)0x/i, "$1"), 16) : parseInt(text, 10);
  return Number.isFinite(value) ? value : fallback;
}

export function attrPair(attrs, key, fallback = [0, 0]) {
  const text = attrs?.[key.toLowerCase()];
  if (!text) return fallback;
  // An empty half (e.g. `size=150,`) reads as 0.
  const parts = text.split(",").map(part => (part.trim() === "" ? 0 : parseInt(part, 10)));
  return parts.length === 2 && parts.every(Number.isFinite) ? parts : fallback;
}

// `0xAARRGGBB` -> {a, r, g, b} (0..255 each).
export function parseColor(text, fallback = 0xffffffff) {
  let value = fallback >>> 0;
  if (text !== undefined && text !== "") {
    const parsed = /^0x/i.test(text.trim()) ? parseInt(text.trim().slice(2), 16) : parseInt(text, 10);
    if (Number.isFinite(parsed)) value = parsed >>> 0;
  }
  return { a: (value >>> 24) & 255, r: (value >>> 16) & 255, g: (value >>> 8) & 255, b: value & 255 };
}

export function cssColor({ a, r, g, b }, alphaScale = 1) {
  return `rgba(${r},${g},${b},${((a / 255) * alphaScale).toFixed(3)})`;
}

// Fit the 640x480 front end into a viewport, keeping 4:3 (black bars like the original on
// wide screens, docs/ui-reference/main-menu.png).
export function fitViewport(width, height) {
  const scale = Math.min(width / FE_WIDTH, height / FE_HEIGHT);
  const w = FE_WIDTH * scale;
  const h = FE_HEIGHT * scale;
  return { scale, x: (width - w) / 2, y: (height - h) / 2, width: w, height: h };
}

export function toLayoutPoint(view, px, py) {
  return [(px - view.x) / view.scale, (py - view.y) / view.scale];
}

export function inRect(rect, x, y) {
  return x >= rect.x && y >= rect.y && x < rect.x + rect.w && y < rect.y + rect.h;
}

// Transition of a control while its screen comes on: p runs 512 -> 0 over 0.5 s after a
// 3-frame delay [VA 0x4e09e0]; slides move by the screen size * p/512 [VA 0x4c8760],
// FADE sets alpha 255 - 255*p/512 [VA 0x4c8840]. Names: VA 0x4c88c0.
export const TRANSITION_MS = 500;
export function transitionState(name, elapsedMs) {
  const p = elapsedMs <= 0 ? 1 : Math.max(0, 1 - elapsedMs / TRANSITION_MS);
  switch ((name || "").toUpperCase()) {
    case "FADE": return { alpha: 1 - p, dx: 0, dy: 0 };
    case "LEFT": return { alpha: 1, dx: -FE_WIDTH * p, dy: 0 };
    case "RIGHT": return { alpha: 1, dx: FE_WIDTH * p, dy: 0 };
    case "TOP": return { alpha: 1, dx: 0, dy: -FE_HEIGHT * p };
    case "BOTTOM": return { alpha: 1, dx: 0, dy: FE_HEIGHT * p };
    default: return { alpha: 1, dx: 0, dy: 0 };
  }
}

// Screen levels after FE_GotoLayout(name, offset) [VA 0x4d2100]: the FE keeps a vector of
// levels (+0xd8); level 0 is the full screen, higher levels are pop-ups drawn over it
// [VA 0x4d2f30]. offset > 0 opens a new pop-up level; 0 (or < 0 with one level) goes to the
// screen in the top level, keeping its history; < 0 closes the top pop-up level first and
// then goes to the screen in the level below. (History kept per level: Hypothesis.)
export function nextLevels(levels, name, offset) {
  const next = levels.map(level => level.slice());
  if (offset > 0 || next.length === 0) next.push([name]);
  else if (offset === 0 || next.length < 2) next[next.length - 1].push(name);
  else {
    next.pop();
    next[next.length - 1].push(name);
  }
  return next;
}

// GOBACK: previous screen of the top level, or close the top pop-up (Hypothesis).
export function backLevels(levels) {
  const next = levels.map(level => level.slice());
  const top = next[next.length - 1];
  if (top && top.length > 1) top.pop();
  else if (next.length > 1) next.pop();
  else return null;
  return next;
}

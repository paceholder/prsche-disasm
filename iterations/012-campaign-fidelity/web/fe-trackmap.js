// Track pictures of the front end from FEData/TrackArt (docs/re/frontend.md "Track pictures").
// Pure module (no DOM): picks shapes and positions, drawing is in frontend.js.
//
// `<track>Sp.fsh` (record +0x44): shapes 0..4 are the 256x192 tiles of the ID_TRACK photo wall
// (tile 0 is the map with the yellow route, 1..4 photos), the last two are chequered-flag
// markers. `<track>SpSm.fsh` (record +0x58): shape 0 is the small map, then two flag markers.
// A shape's header centre (cx, cy) is where the marker sits: inside the tile for Sp, in screen
// coordinates for SpSm [data TrackArt/*.fsh headers].

// Track1.Direction indices: 0 forward, 1 backward, 2 forward mirrored, 3 backward mirrored.
export const isBackward = direction => (direction & 1) === 1;
export const isMirrored = direction => direction >= 2;

// The flag shown: the last shape (F, `fin`) going backward, the one before it (S, `star`)
// going forward. The wall code picks pack entry +0x44 (shape 6) when the setup's reverse flag
// is set, else +0x3c (shape 5) [VA 0x4eada5]; the reference frame of the 911 Cup (first race
// Cote d'Azur, reverse set in nfs5.trn) agrees for the small map.
function flagShape(shapes, direction) {
  const n = shapes.length;
  return shapes[isBackward(direction) ? n - 1 : n - 2] ?? null;
}

// Wall tile 0 with its flag. Marker position = tile origin + (cx, cy). `flipX` mirrors the
// whole picture for mirrored directions (the wall flips the picture, docs/re/frontend.md).
export function wallMap(shapes, direction) {
  if (!shapes || shapes.length < 3 || !shapes[0]?.canvas) return null;
  const tile = shapes[0];
  const flag = flagShape(shapes, direction);
  return {
    tile,
    flipX: isMirrored(direction),
    flag: flag?.canvas ? { shape: flag, x: flag.cx, y: flag.cy } : null,
  };
}

// Small map of the Evolution event panel and its flag, both at their header centres (screen
// coordinates).
export function smallMap(shapes, direction) {
  if (!shapes || shapes.length < 3 || !shapes[0]?.canvas) return null;
  const map = shapes[0];
  const flag = flagShape(shapes, direction);
  return {
    map, x: map.cx, y: map.cy,
    flag: flag?.canvas ? { shape: flag, x: flag.cx, y: flag.cy } : null,
  };
}

// Where the wall picture is drawn on the 640x480 screen, measured on the reference frame of
// the original (docs/ui-reference/location-original-youtube.png): the bezel is centred on the
// porthole at x 320, the picture 300x225 (the 256x192 tile at 1.17), the bezel about 12 px wide.
export const WALL_CENTER = [320, 278];
export const WALL_SIZE = [300, 225];
export const WALL_BEZEL = 12;
export const SCAN_PERIOD = 19;

// ID_TRACK wall timing [VA 0x4eaea6..0x4eaf24, 0x4eac6b..0x4ead00]: the wall waits 0x200 clock
// ticks, then turns for 1/0x5b2828 = 64 ticks (phase = elapsed * 0.015625), then the next
// face shows texture index + 1 modulo the tile count (index 0 = the map; the flag is drawn
// only on it). A track or direction change restarts at tile 0.
export const WALL_IDLE_TICKS = 512;
export const WALL_TURN_TICKS = 64;
// The clock 0x6b7c7c counts main-loop ticks; its rate is not recovered. 60 per second is
// assumed (A220).
export const WALL_TICKS_PER_SECOND = 60;

// Tile shown at `ticks` since the last change: { from, to, turn } where turn 0 = still on
// `from`, 0..1 = turning to `to`.
export function wallCycle(ticks, tileCount) {
  const n = Math.max(1, tileCount);
  const period = WALL_IDLE_TICKS + WALL_TURN_TICKS;
  const t = Math.max(0, ticks);
  const k = Math.floor(t / period);
  const phase = t - k * period;
  const from = k % n;
  const turn = phase < WALL_IDLE_TICKS ? 0 : (phase - WALL_IDLE_TICKS) / WALL_TURN_TICKS;
  return { from, to: (from + 1) % n, turn };
}

// Number of photo tiles of a Sp pack: everything but the two flag shapes.
export const tileCount = shapes => Math.max(0, (shapes?.length ?? 0) - 2);

// Record time in 1/64 s (nfs5.trk +0x1f8) as "mm:ss.cc" [VA 0x4356e0; reference frame 02:44.00].
export function formatRecordTime(units) {
  if (!Number.isFinite(units) || units <= 0) return "";
  const hundredths = Math.floor((units * 100) / 64);
  const pad = n => String(n).padStart(2, "0");
  return `${pad(Math.floor(hundredths / 6000))}:${pad(Math.floor(hundredths / 100) % 60)}.${pad(hundredths % 100)}`;
}

// FETrackSmall2DSpline update [VA 0x4fcb40]: with t = clock ticks since the event was shown,
// the map index is t >> 8 (over the event's races, back to 0 past the last), and the map's
// alpha ramps t&255 * 8 over the first 32 ticks of each slot and 0x7ff - 8*t over the last
// 32 (0 at the end), else full.
export const SPLINE_SLOT_TICKS = 256;
export function splineCycle(ticks, raceCount) {
  const n = Math.max(1, raceCount);
  const t = Math.max(0, Math.floor(ticks));
  const index = Math.floor(t / SPLINE_SLOT_TICKS) % n;
  const m = t % SPLINE_SLOT_TICKS;
  let alpha = 255;
  if (m < 0x20) alpha = m * 8;
  if (m > 0xe0) alpha = Math.max(0, Math.min(255, 0x7ff - m * 8));
  return { index, alpha };
}

// Colours of the spline control [VA 0x4d87c0, 0x4d87d0]: yellow 0xffbb00 for the map and the
// current location, white for the flag and the other locations, alpha in the top byte.
export const SPLINE_YELLOW = [0xff, 0xbb, 0x00];
export const SPLINE_WHITE = [0xff, 0xff, 0xff];

// Clicks on the 3D car of the front end (`FECar3DControl` 0x4c0eb0, zone function 0x4c0d10).
// The zone depends on the room's yaw (turns, the fraction of vtbl +0xb4) and the click position
// inside the control's rectangle. Zones: 0 cabrio top, 1/2 doors (sound only in the original),
// 3 hood, 4 trunk lid.

// Port of 0x4c0d10. `rect` = {left, top, right, bottom} of the control, `hasTop`: the car has a
// top channel (`FUN_00438190(1) != 0`). Integer divisions follow the C code.
export function carClickZone(x, y, rect, yawTurns, hasTop) {
  const { left, top, right, bottom } = rect;
  const h = (bottom - top) * 5;
  if (hasTop && y <= (h + ((h >> 31) & 15) >> 4) + top) return 0;
  const f = yawTurns - Math.trunc(yawTurns);
  const side = Math.trunc((right - left) / 6);
  let zone = 0;
  if (f > 0.9 || f <= 0.1) zone = 4;
  if (f > 0.4 && f <= 0.6) zone = 3;
  if (f > 0.6 && f <= 0.9) {
    if (x < side + left) zone = 3;
    else zone = x <= right - side ? 1 : 4;
  }
  if (f > 0.1 && f <= 0.4) {
    if (x < side + left) return 4;
    return right - side < x ? 3 : 2;
  }
  return zone;
}

// Channels moved by a click: top (frame channel 1), doors (transform channels 6 and 7),
// hood 8, trunk 9.
// Zone 1 -> channel 6, zone 2 -> channel 7 (`0x4c0520`: levels +0x3a8 / +0x3ac); on screen channel 6
// is the near door when the car's front points left, the side zone 1 covers.
export const ZONE_CHANNEL = { 0: 1, 1: 6, 2: 7, 3: 8, 4: 9 };

// Opening time in ms of the part tweens. `0x4c0520` runs a level 0..255 from the 128 Hz tick
// count since the click: top (2 * ticks / 3) 2.99 s, doors ticks / 0.302 open and ticks / 0.389
// close (0.60 s, 0.78 s); the hood and trunk lids tween over 1000 ms (0x439a00).
export function carPartMs(channel, opening) {
  switch (channel) {
    case 1: return (255 * 3 / 2) / 128 * 1000;
    case 6: case 7: return 255 * (opening ? 0.302 : 0.389) / 128 * 1000;
    default: return 1000;
  }
}

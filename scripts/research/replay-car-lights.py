"""Record the original car light groups and the pop-up headlight channel by executing nfs5.exe x86 code.

Runs unchanged in Unicorn, on a synthetic R3DCar and vehicle (no game data is read):
  - R3DCar_UpdateLights 0x439360 (vtable 0x5b3620 slot +0x38): the 14 light groups ("lit"
    bytes `R3DCar+0x2d4`, levels `R3DCar+0x2d8`, setters 0x443670 / 0x4436a0), the headlight
    animator at `R3DCar+0x64` (+0x110 start, +0x114 duration, +0x118 from, +0x11c to) and the
    switch latch `R3DCar+0x567`; `R3DCar+0x70` is the TPG `headlightextent` (read by the
    R3DCar setup at 0x441cf5) in turns, 0 for cars without retractable lamps.
  - R3DCar_SetAnimChannels 0x438440 with car detail `[0x6197c4]` = 0 (it returns after the
    lid channels, before the driver frame sequences): transform channel 0x11 x
    (`R3DCar+0x2d0 + 0x11 * 12`), the lamp-lid rotation in turns, from the same animator.
The only stub is the millisecond clock 0x53bfd0 (an import thunk, GetTickCount), answered with
the scripted time of the step; the tick counter `[0x6b7c7c]` (0x53c2e0) and the tick rate
`[0x5deb48]` are written as globals. Notes: docs/re/lighting.md in the workspace root.

    python3 scripts/research/replay-car-lights.py \
        --out-dir iterations/012-campaign-fidelity/crates/nfs-assets/tests/fixtures

Writes car-lights-steps.tsv: one row per step (inputs, then the original outputs).
"""
import argparse
from pathlib import Path
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from vehicle_harness import CAR, ROOT, SCRATCH, Machine, f32, fmt, load_module, write_tsv  # noqa: E402

UPDATE_LIGHTS = 0x439360
SET_ANIM_CHANNELS = 0x438440
CLOCK_MS = 0x53bfd0          # jmp [0x5b2080] (GetTickCount)

R3D = SCRATCH                # synthetic R3DCar (0x700 bytes)
ANIM = SCRATCH + 0x800       # animator block `R3DCar+0x64` (0x200 bytes)
LIT = SCRATCH + 0xa00        # `R3DCar+0x2d4` group lit bytes
LEVEL = SCRATCH + 0xa40      # `R3DCar+0x2d8` group levels (f32)
CHANNELS = SCRATCH + 0xb00   # `R3DCar+0x2d0` transform channels, 12 bytes each

G_SHOWROOM = 0x65b928
G_TICKS = 0x6b7c7c
G_TICK_RATE = 0x5deb48
TICK_RATE = 128              # any nonzero rate; the test derives the blink phase the same way
GLOBALS = {G_SHOWROOM: 0, G_TICKS: 0, G_TICK_RATE: TICK_RATE, 0x6197c4: 0, 0x65742c: 0, 0x657424: 0}

ZONES = [0x4e4 + 4 * i for i in range(9)]   # car+0x4e4[0..9] zone damage


class LightMachine(Machine):
    def __init__(self, module, data):
        super().__init__(module, data, stubs={CLOCK_MS: lambda m: m.now_ms})
        self.now_ms = 0

    def setup(self, extent, showroom):
        self.reset_car()
        self.uc.mem_write(R3D, bytes(0xc00))
        self.put_u32(R3D + 0xf4, CAR)
        self.put_u32(R3D + 0x64, ANIM)
        self.put(R3D + 0x70, extent)
        self.put_u32(R3D + 0x2d0, CHANNELS)
        self.put_u32(R3D + 0x2d4, LIT)
        self.put_u32(R3D + 0x2d8, LEVEL)
        self.put(LEVEL, *([1.0] * 14))
        for addr, value in GLOBALS.items():
            self.put_u32(addr, value)
        self.put_u32(G_SHOWROOM, int(showroom))

    def step(self, s):
        self.now_ms = s['ms']
        self.put_u32(G_TICKS, s['ticks'])
        car = self.uc
        car.mem_write(CAR + 0xd7f, bytes([s['gas'], s['brake']]))
        car.mem_write(CAR + 0xd82, bytes([s['gear']]))
        car.mem_write(CAR + 0xd87, bytes([s['switches']]))
        self.put_u32(CAR + 0xd98, s['reverse_d98'])
        self.put_u32(CAR + 0xb50, s['b50'])
        for off, v in zip(ZONES, s['damage']):
            self.put(CAR + off, v)
        self.call_thiscall(UPDATE_LIGHTS, R3D)
        self.call_thiscall(SET_ANIM_CHANNELS, R3D)
        lit = self.get_bytes(LIT, 14)
        level = self.get(LEVEL, 14)
        start, duration = struct.unpack('<II', self.get_bytes(ANIM + 0x110, 8))
        frm, to = self.get(ANIM + 0x118, 2)
        return {
            'lit': ''.join(str(b) for b in lit),
            'level': ','.join(fmt(f32(v)) for v in level),
            'anim_start': start, 'anim_dur': duration, 'anim_from': frm, 'anim_to': to,
            'latch': self.get_bytes(R3D + 0x567, 1)[0],
            'ch11': self.get(CHANNELS + 0x11 * 12, 1)[0],
        }


def base_step(ms):
    return {'ms': ms, 'ticks': ms * TICK_RATE // 1000, 'gas': 0, 'brake': 0, 'gear': 2,
            'switches': 0, 'reverse_d98': 0, 'b50': 0, 'damage': [0.0] * 9}


def schedule(events, end_ms, dt=20, start_ms=5000):
    """Steps every `dt` ms; `events` = [(ms, {field: value})] applied from that time on."""
    steps, state = [], {}
    for ms in range(0, end_ms + 1, dt):
        for at, change in events:
            if at == ms:
                state.update(change)
        s = base_step(start_ms + ms)
        s.update(state)
        steps.append(s)
    return steps


def cases():
    on, off = {'switches': 0x3}, {'switches': 0}
    toggles = [(100, on), (2000, off), (3500, on), (3900, off), (5000, on)]
    for name, extent in (('fixed_lamps', 0.0), ('popup_944', 0.14), ('popup_914', 0.19),
                         ('popup_928', -0.18)):
        yield name, extent, False, schedule(toggles, 6500)
    # Lights on while braking, indicators, hazard, reverse, then broken zones.
    yield 'popup_mixed', 0.14, False, schedule([
        (0, on), (600, {'brake': 200}), (900, {'brake': 0, 'switches': 0x3 | 0x8}),
        (1500, {'switches': 0x3 | 0x10}), (2100, {'b50': 3}), (2500, {'b50': 0, 'gear': 0}),
        (2800, {'gear': 2, 'reverse_d98': 1, 'gas': 90, 'brake': 0}),
        (3000, {'reverse_d98': 0, 'gas': 0}),
        (3200, {'damage': [31.0, 0.0, 30.0, 0.0, 45.0, 0.0, 0.0, 0.0, 0.0]}),
        (3600, {'damage': [0.0, 0.0, 50.0, 0.0, 0.0, 31.0, 60.0, 0.0, 99.0]}),
        (4200, off),
    ], 5600)
    yield 'fixed_mixed', 0.0, False, schedule([
        (0, {'brake': 255}), (400, {'brake': 0, 'switches': 0x8}), (800, {'switches': 0x3}),
        (1200, {'brake': 10}), (1600, {'brake': 0, 'switches': 0x11}), (2400, off),
    ], 3000)
    yield 'showroom', 0.14, True, schedule([
        (0, on), (500, {'damage': [40.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]}),
    ], 1000, dt=100)


INPUTS = ['ms', 'ticks', 'gas', 'brake', 'gear', 'switches', 'reverse_d98', 'b50']


def record_all(module, data):
    m = LightMachine(module, data)
    rows = []
    for name, extent, showroom, steps in cases():
        m.setup(extent, showroom)
        for i, s in enumerate(steps):
            out = m.step(s)
            row = [('case', name), ('step', i), ('extent', f32(extent)), ('showroom', int(showroom))]
            row += [(k, s[k]) for k in INPUTS]
            row += [('damage', ','.join(fmt(f32(v)) for v in s['damage']))]
            row += list(out.items())
            rows.append(row)
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    parser.add_argument('--module', default='nfs5.exe')
    parser.add_argument('--game-dir', type=Path, default=ROOT / 'local/game')
    parser.add_argument('--out-dir', type=Path, required=True)
    args = parser.parse_args()
    module, data = load_module(args.module, args.game_dir)
    runs = [record_all(module, data) for _ in range(2)]
    if runs[0] != runs[1]:
        raise RuntimeError('non-deterministic recording')
    write_tsv(args.out_dir / 'car-lights-steps.tsv', runs[0])
    print(f'car-lights-steps.tsv: {len(runs[0])} rows')


if __name__ == '__main__':
    main()

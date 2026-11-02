"""Record the original race HUD gauge values: the speed word car+0xec4
(`Car_UpdateSpeedEc4` 0x40eec0) and the gauge updates that read it, plus the
tachometer.

Executes nfs5.exe in Unicorn (render_harness.RenderMachine; the driver image is
mapped but not used) on synthetic inputs:

- 0x40eec0, cdecl (car): car+0xec4 = max + 0.25 * min of |car+0x33c|,
  |car+0x344|.
- HSpeedometer_Update 0x42e840 (thiscall): needle value
  ftol(car+0xec4 * [0x5b3018] (km/h) | [0x5b3014] (mph, [0x657dc0] != 0)),
  passed to 0x421650 (stubbed, the value is the output).
- HTach ctor tail 0x42f501..0x42f532: this+0xa9d0 = (R - 750) / R with R the
  int car+0xee0 (fild / fidiv); HTach_Update 0x42f660 (thiscall): needle value
  ftol(car+0xdb4 * this+0xa9d0 + [0x5b30d8]) / 10 (signed divide).
- HSpeed_Update 0x42e570 and HTachDigital_Update 0x42f740 (thiscall): the
  text built at this+0xd4 (padding strings, `_itoa` 0x5a19ac) and the label id
  passed to 0x41e160 (stubbed, as 0x536da0 and 0x41c8c0).

The visibility checks 0x41e920/0x41e940/0x41e960/0x41e980 and 0x42a3b0 are
stubbed to return 1. The car is CAR = player slot [0x5e8e2c + 4 * [0x65741c]].
x87 control word 0x27f (53-bit precision, the CRT start-up state).

    python scripts/research/replay-hud-gauges.py \
        --out iterations/012-campaign-fidelity/crates/nfs-assets/tests/fixtures/hud-gauges.tsv
"""
import argparse
import math
from pathlib import Path
import random
import struct
import sys

from unicorn.x86_const import UC_X86_REG_ESI, UC_X86_REG_EDI, UC_X86_REG_ESP

sys.path.insert(0, str(Path(__file__).resolve().parent))
import render_harness as rh  # noqa: E402
from render_harness import DATA  # noqa: E402

SPEED_EC4, SPEEDO, TACH, SPEED_TEXT, RPM_TEXT = 0x40eec0, 0x42e840, 0x42f660, 0x42e570, 0x42f740
TACH_K_BEGIN, TACH_K_END = 0x42f501, 0x42f532
CAR, WIDGET = DATA + 0x10000, DATA + 0x20000
CAR_TABLE, PLAYER, UNITS = 0x5e8e2c, 0x65741c, 0x657dc0
VISIBLE = (0x41e920, 0x41e940, 0x41e960, 0x41e980, 0x42a3b0)


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def bits(x):
    return format(struct.unpack('<I', struct.pack('<f', x))[0], '08x')


class Recorder:
    def __init__(self, game_dir):
        self.m = m = rh.RenderMachine(game_dir)
        self.out = {}
        for va in VISIBLE:
            self._stub(va, 0, lambda m, a: 1)

        def needle(m, a):
            self.out['needle'] = a[0] - (1 << 32) if a[0] & 0x80000000 else a[0]
        self._stub(0x421650, 1, needle)

        # 0x41e160 (stdcall, label id) -> label string; 0x536da0 (cdecl, label, text) -> the
        # joined string; 0x41c8c0 (thiscall, string) shows it.
        def label(m, a):
            self.out['label'] = a[0]
            return DATA + 0x100

        def text(m, a):
            self.out['text'] = m.get_bytes(a[1], 32).split(b'\0')[0].decode()
            return DATA + 0x200
        self._stub(0x41e160, 1, label)
        self._stub(0x536da0, 0, text)
        self._stub(0x41c8c0, 1, lambda m, a: 0)

    def _stub(self, va, pop, fn):
        """Stub a function of the image: `pop` stack dwords for the callee (thiscall/stdcall);
        the handler sees the first four stack arguments."""
        def handler(m, args):
            sp = m.uc.reg_read(UC_X86_REG_ESP)
            a = list(struct.unpack('<4I', m.get_bytes(sp + 4, 16)))
            return fn(m, a)
        self.m.stubs[va] = ('stub', pop, handler)

    def car(self, **fields):
        m = self.m
        m.put_bytes(CAR, bytes(0x1000))
        m.put_bytes(WIDGET, bytes(0xb000))
        m.put_u32(PLAYER, 0)
        m.put_u32(CAR_TABLE, CAR)
        for off, kind, value in fields.values():
            m.uc.mem_write(CAR + off, struct.pack(kind, value))

    def speed_ec4(self, vx, vz):
        self.car(vx=(0x33c, '<f', vx), vz=(0x344, '<f', vz))
        self.m.call(SPEED_EC4, CAR)
        return self.m.get_f32(CAR + 0xec4)

    def speedo(self, ec4, units, text=False):
        self.car(ec4=(0xec4, '<f', ec4))
        self.m.put_u32(UNITS, units)
        self.out = {}
        self.m.call(SPEED_TEXT if text else SPEEDO, ecx=WIDGET)
        return self.out

    def tach_k(self, redline):
        """The ctor tail: this+0xa9d0 from car+0xee0 (run from 0x42f501 to 0x42f532)."""
        m = self.m
        self.car(red=(0xee0, '<i', redline))
        sp = rh.STACK_TOP - 0x1000
        m.put_bytes(sp, bytes(0x40))
        m.uc.reg_write(UC_X86_REG_ESP, sp)
        m.uc.reg_write(UC_X86_REG_ESI, WIDGET)
        m.uc.reg_write(UC_X86_REG_EDI, 0)
        from unicorn.x86_const import UC_X86_REG_FPCW
        m.uc.reg_write(UC_X86_REG_FPCW, m.fpcw)
        m.uc.emu_start(TACH_K_BEGIN, TACH_K_END, count=1000)
        return m.get_f32(WIDGET + 0xa9d0)

    def tach(self, rpm, k, text=False):
        self.car(rpm=(0xdb4, '<f', rpm))
        self.m.uc.mem_write(WIDGET + 0xa9d0, struct.pack('<f', k))
        self.out = {}
        self.m.call(RPM_TEXT if text else TACH, ecx=WIDGET)
        return self.out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--game-dir', type=Path, default=None)
    ap.add_argument('--out', type=Path, required=True)
    args = ap.parse_args()
    rec = Recorder(args.game_dir)
    rnd = random.Random(0x40eec0)
    rows = []

    def row(kind, a, b, out):
        rows.append((kind, a, b, out))

    # 0x40eec0: signs, zeros, equal magnitudes, small and large speeds.
    special = [0.0, -0.0, 1.0, -1.0, 0.25, 3.0, -3.0, 27.7778, 100.0, -100.0, 1e-30, 1e30]
    pairs = [(a, b) for a in special for b in special]
    pairs += [(f32(rnd.uniform(-90, 90)), f32(rnd.uniform(-90, 90))) for _ in range(400)]
    for vx, vz in pairs:
        row('ec4', bits(vx), bits(vz), bits(rec.speed_ec4(vx, vz)))
    # Speedometer needle and digital text on the same speeds (ec4 >= 0).
    speeds = [0.0, 0.5, 0.99999994, 1.0, 2.7777777, 2.7777779, 2.8, 27.7777, 27.77778, 27.8,
              33.3, 50.0, 69.4, 83.3, 90.0, 120.0, 277.8, 300.0]
    speeds += [f32(rnd.uniform(0, 100)) for _ in range(300)]
    speeds += [f32(n / 3.6) for n in range(0, 400)] + [f32(n / 2.2374139) for n in range(0, 250)]
    for s in speeds:
        s = f32(s)
        for units in (0, 1):
            needle = rec.speedo(s, units)['needle']
            t = rec.speedo(s, units, text=True)
            row('speedo', bits(s), units, f"{needle}|{t['label']}|{t['text']}")
    # Tachometer: the ctor's factor for redlines, then the needle and the digital text.
    redlines = [751, 1000, 5500, 5800, 6000, 6200, 6500, 6800, 7000, 7200, 7500, 8000, 8500, 9000]
    for r in redlines:
        k = rec.tach_k(r)
        row('tach_k', r, '-', bits(k))
        rpms = [0.0, 1.0, 9.9, 10.0, 99.9, 100.0, 750.0, 999.9, 1000.0, float(r), float(r) + 0.5]
        rpms += [f32(rnd.uniform(0, r * 1.05)) for _ in range(60)]
        for rpm in rpms:
            rpm = f32(rpm)
            needle = rec.tach(rpm, k)['needle']
            t = rec.tach(rpm, k, text=True)
            row('tach', bits(rpm), r, f"{needle}|{t['label']}|{t['text']}")
    lines = ['# replay-hud-gauges.py: nfs5.exe 0x40eec0, 0x42e840, 0x42e570, 0x42f501..0x42f532, '
             '0x42f660, 0x42f740 (synthetic inputs, f32 as bits)',
             '# ec4: a = vx, b = vz, out = car+0xec4; speedo: a = car+0xec4, b = units, '
             'out = needle|label|text; tach_k: a = car+0xee0, out = +0xa9d0; '
             'tach: a = car+0xdb4, b = redline, out = needle|label|text',
             'kind\ta\tb\tout']
    lines += ['\t'.join(str(c) for c in r) for r in rows]
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{args.out}: {len(rows)} rows')


if __name__ == '__main__':
    main()

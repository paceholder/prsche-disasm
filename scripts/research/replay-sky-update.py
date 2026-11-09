"""Record the original per-frame update of the MULTI* sky layers: `SkyManager_Update` 0x45d030
(the frame's tick count from the 64 Hz `g_SchedStep [0x606ac4]`, the camera vector turned into
the dome frame, the weather blend) -> layer vtable 0x5b3ea0 slot 0 0x45bce0 (map mix 0x45df50
-> 0x45e160, UV grid update 0x45f0a0: A/B blend 0x45f180 / 0x45f160, drift and camera offset
0x45ed40, warp 0x45ee80 with the game generator 0x5561d0 -> 0x538040, UVs 0x45ec40).

Executes nfs5.exe in Unicorn (render_harness.RenderMachine) on layers built by the real
`SkyMapLayer_Ctor` 0x45b5a0 (replay-sky-layer.py harness: synthetic SHPI and txt blocks; the
build's own warp start 0x45e8e0 -> 0x45f3c0 runs too). Synthetic inputs only, the fixture is
committed.

Per case the generator state [0x5de910..0x5de924] is set to a synthetic value before the build
(recorded in the `case` row). A one-layer manager object is made by hand: +0 the layer, +0x28
count 1, +0x34 the track (sky config +0x104), +0x38 / +0x40 the camera x / z (what
`SkyManager_Draw` 0x45d4f0 stores from the camera at +0x10c / +0x114), +0x44 a one-key table
for the sky-map blend 0x45a350 (key value = the frame's blend), +0x48 a one-key table for
0x45a560 / 0x45a910, +0x50 node 0, +0x5c the last step (0 = first frame). Stubbed (harness
services, after the layer update): the lightning flash 0x45ccf0; [0x627e38] points at a zero
byte (no storm: the map object mixes A/B with 0x45e160).

Rows per frame: `frame` (g_SchedStep, camera, blend; the arguments the layer receives: the
two camera vectors, blend, ticks) and `grid` (per layer drift offset +0x40, camera offset
+0x28, 1/scale blend +0xec, the UVs of 16 vertices; and when the warp flag +0x74 is set the
256 warp offsets +0x5c and the warp counters). `start` rows: the grid fields set by 0x45f1b0
(+0x7c..+0x90, +0x124..+0x12c, flags +0x70..+0x74) and the warp offsets after the build.

    python scripts/research/replay-sky-update.py \
        --out iterations/012-campaign-fidelity/crates/nfs-assets/tests/fixtures/sky-update.tsv
"""
import argparse
import importlib.util
from pathlib import Path
import random
import struct
import sys

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import render_harness as rh  # noqa: E402

_spec = importlib.util.spec_from_file_location('replay_sky_layer', HERE / 'replay-sky-layer.py')
sl = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(sl)

UPDATE, LAYER_UPDATE, FLASH = 0x45d030, 0x45bce0, 0x45ccf0
SCHED_STEP, RNG, STORM = 0x606ac4, 0x5de910, 0x627e38
D = rh.DATA
MGR, KEYS_A, KEYS_B, TABLE_A, TABLE_B, STORM_OBJ = (D + 0x26000, D + 0x26100, D + 0x26200, D + 0x26300,
                                                   D + 0x26400, D + 0x26500)
SAMPLE = [0, 1, 7, 15, 16, 17, 40, 85, 119, 136, 170, 200, 238, 240, 254, 255]


def bits(x):
    return sl.bits(x)


def hexf(m, addr, n):
    return ','.join(f'{v:08x}' for v in struct.unpack(f'<{n}I', m.get_bytes(addr, 4 * n)))


class UpdateRecorder(sl.Recorder):
    def __init__(self, game_dir):
        super().__init__(game_dir)
        m = self.m
        self.rng = None
        self.args = None
        m.stubs[FLASH] = ('flash', 2, lambda m_, a: 0)

        def on_layer(uc, _a, _s, _u):
            sp = uc.reg_read(UC_X86_REG_ESP)
            va, vb, blend, ticks = struct.unpack('<4I', uc.mem_read(sp + 4, 16))
            self.args = (m.get_bytes(va, 12), m.get_bytes(vb, 12), blend, struct.unpack('<i', struct.pack('<I', ticks))[0])
        m.uc.hook_add(UC_HOOK_CODE, on_layer, begin=LAYER_UPDATE, end=LAYER_UPDATE)

    def reset(self):
        super().reset()
        if self.rng is not None:
            self.m.put_bytes(RNG, struct.pack('<6I', *self.rng))

    def setup_manager(self, layer):
        m = self.m
        m.put_bytes(MGR, bytes(0x80))
        m.put_u32(MGR, layer)
        m.put_u32(MGR + 0x28, 1)
        m.put_u32(MGR + 0x34, sl.TRACK)
        m.put_u32(MGR + 0x44, KEYS_A)
        m.put_u32(MGR + 0x48, KEYS_B)
        m.put_u32(MGR + 0x50, 0)
        for keys, table, size in ((KEYS_A, TABLE_A, 8), (KEYS_B, TABLE_B, 0x30)):
            m.put_bytes(keys, bytes(0x20))
            m.put_bytes(table, bytes(size))
            m.put_u32(keys, table)
        m.put_u32(KEYS_A + 8, 1)       # 0x45a350: count at [2], current keys [3], [4]
        m.put_u32(KEYS_B + 4, 1)       # 0x45a560: count at [1], current keys [2], [3]
        m.put_bytes(STORM_OBJ, bytes(16))
        m.put_u32(STORM, STORM_OBJ)

    def frame(self, step, cam, blend):
        m = self.m
        m.put_i32(SCHED_STEP, step)
        m.put_bytes(MGR + 0x38, struct.pack('<f', cam[0]))
        m.put_bytes(MGR + 0x40, struct.pack('<f', cam[1]))
        m.put_bytes(TABLE_A + 4, struct.pack('<f', blend))
        self.args = None
        m.call(UPDATE, ecx=MGR, budget=20_000_000)
        return self.args

    def grid_rows(self, cid, key, layer, warp_dump):
        m = self.m
        g = m.get_u32
        grid = g(layer + 0x40)
        rows = []
        for i in range(m.get_i32(grid + 0x20)):
            uvp = g(grid + 4 + 4 * i)
            uv = ','.join(m.get_bytes(uvp + 8 * v, 8).hex() for v in SAMPLE)
            rows.append(('grid', cid, f'{key}.{i}', f'offset={hexf(m, grid + 0x40 + 8 * i, 2)};'
                         f'camoff={hexf(m, grid + 0x28 + 8 * i, 2)};inv={hexf(m, grid + 0xec + 0x18 * i, 2)};uv={uv}'))
        if warp_dump:
            rows.append(('warp', cid, key, f'pos={m.get_bytes(g(grid + 0x5c), 256 * 8).hex()};'
                         f'acc={m.get_i32(grid + 0x130)},{m.get_i32(grid + 0x134)};cursor={m.get_i32(grid + 0x16c)};'
                         f'r={hexf(m, grid + 0x138, 13)};rng={hexf(m, RNG, 6)}'))
        return rows

    def start_row(self, cid, layer):
        m = self.m
        grid = m.get_u32(layer + 0x40)
        flags = ','.join(str(b) for b in m.get_bytes(grid + 0x70, 5))
        return ('start', cid, '-', f'f7c={hexf(m, grid + 0x7c, 6)};f124={hexf(m, grid + 0x124, 3)};flags={flags};'
                f'counts={m.get_i32(grid + 0x58)},{m.get_i32(grid + 0x60)},{m.get_i32(grid + 0x68)};'
                f'acc={m.get_i32(grid + 0x130)},{m.get_i32(grid + 0x134)};cursor={m.get_i32(grid + 0x16c)};'
                f'r={hexf(m, grid + 0x138, 13)};rng={hexf(m, RNG, 6)}')


def cases():
    base = {c['name']: c for c in sl.cases()}
    images = base['one-map-flat']['images']
    dome_a = (0.62, 1500.0, 0.0002, 44.4, 0.236)
    dome_b = (0.0, 1000.0, 0.0002, 125.0, 1.5)
    out = []

    def case(name, ra, rb, clouds, dome=dome_a):
        out.append(dict(name=name, tokens=['no'] + ra + rb + clouds, images=images, dome=dome, mode=0, flags=4))
    r = sl.region
    # Farmland-like: camera offset (ratio 400), warp (0.2 / 0.1), wind on the cloud layer.
    a = r(['m1aa', 'm2aa', 'none'], [1, 1.5, 0], [1, 2, 0], 400, [0, 3, 0], [90, 90, 0], 0.2, 0.1)
    case('farmland-like', a, a, ['flat', 'c1aa', 'none'], dome_b)
    # Region B differs (blend), fast ratio 50, wind on both layers, stronger warp.
    a = r(['m1aa', 'm3aa', 'none'], [2, 3, 0], [2, 4, 0], 50, [1, 6, 0], [10, 200, 0], 0.5, 0.3)
    b = r(['m2aa', 'm5aa', 'none'], [4, 1.5, 0], [1, 5, 0], 25, [2, 9, 0], [190, 20, 0], 0.1, 0.05)
    case('blend-warp', a, b, ['flat', 'c1aa', 'none'])
    # Coastal-like: ratio 400, no warp, wind 0 on the map layer (no move flag), one layer.
    a = r(['m4aa', 'none', 'none'], [2, 5, 0], [6, 1, 0], 400, [0, 6, 0], [90, 0, 0], 0, 0)
    case('coastal-like', a, a, ['flat', 'none', 'none'])
    # Ratio 0 (no camera offset), wind on layer 1 only (Autobahn-like).
    a = r(['m1aa', 'm2aa', 'none'], [1, 2, 0], [1, 2, 0], 0, [0, 3, 0], [90, 40, 0], 0, 0)
    case('autobahn-like', a, a, ['flat', 'c1aa', 'none'], dome_b)
    return out


def schedule(seed):
    """(g_SchedStep, camera x/z, blend) per frame."""
    rnd = random.Random(seed)
    steps, s = [], 1000
    for d in (0, 1, 2, 3, 4, 0, 7, 83, 200, -400, 1, 1, 2, 1, 5):
        s += d
        steps.append(s)
    for _ in range(25):
        s += rnd.choice((0, 1, 1, 1, 2, 2, 3, 4, 5, 9))
        steps.append(s)
    cams = [(0.0, 0.0), (123.5, -77.25), (-4000.0, 2500.5), (10000.75, 3.0), (-0.3, 0.7)]
    out = []
    for k, st in enumerate(steps):
        cam = cams[k % len(cams)] if k < 10 else (rnd.uniform(-3000, 3000), rnd.uniform(-3000, 3000))
        blend = (0.0, 0.25, 1.0, 0.5)[k % 4] if k < 12 else rnd.choice((0.0, 0.3, 0.7, 1.0))
        out.append((st, cam, blend))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--game-dir', type=Path, default=None)
    ap.add_argument('--out', type=Path, required=True)
    args = ap.parse_args()
    rec = UpdateRecorder(args.game_dir)
    rows = []
    for cid, case in enumerate(cases()):
        rnd = random.Random(0x45d030 + cid)
        rec.rng = [rnd.getrandbits(32) for _ in range(6)]
        layer = rec.build(case)
        rec.setup_manager(layer)
        rows.append(('case', cid, case['name'], ' '.join(case['tokens']) + f';rng={",".join(f"{v:08x}" for v in rec.rng)}'
                     f';dome={",".join(bits(v) for v in case["dome"])}'))
        rows.append(rec.start_row(cid, layer))
        warp = rec.m.get_bytes(rec.m.get_u32(layer + 0x40) + 0x74, 1)[0] != 0
        rows += rec.grid_rows(cid, 'build', layer, True)
        for k, (step, cam, blend) in enumerate(schedule(cid)):
            va, vb, b, ticks = rec.frame(step, cam, blend)
            rows.append(('frame', cid, k, f'step={step};cam={bits(cam[0])},{bits(cam[1])};blend={bits(blend)};'
                         f'va={va.hex()};vb={vb.hex()};arg_blend={b:08x};ticks={ticks}'))
            rows += rec.grid_rows(cid, k, layer, warp)
    lines = ['# replay-sky-update.py: nfs5.exe 0x45d030 SkyManager_Update -> 0x45bce0 -> 0x45f0a0 '
             '(synthetic layers; f32 as bits, vectors and UVs as memory bytes)', 'kind\tcase\tkey\tout']
    lines += ['\t'.join(str(c) for c in r) for r in rows]
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{args.out}: {len(rows)} rows')


if __name__ == '__main__':
    main()

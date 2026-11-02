"""Record the original material render state: the `sR` layer block through
`Render_ApplyLayerState` 0x540660 and the THRASH driver dx7z.dll down to the
Direct3D 7 calls, plus the single-texture combine state 0x29 = 8 / 0x10029 = 0
of the draws (0x540ce0, 0x5405f0).

Executes nfs5.exe and Drivers/dx7/dx7z.dll in Unicorn (render_harness.py):

- 0x540660 (stdcall, block pointer): for each of the six fields
  `{shade, blend enable, blend mode, cull, z bias, -1}` compares the value with
  the cache [0x5ded34 + 8k]; on a change stores it and calls the setter
  [0x5ded60 + 4k] (cdecl, THRASH id [0x5ded30 + 8k], value): 0x5405c0 (shade:
  getstate, setstate, restore on failure), THRASH directly (blend enable),
  0x540550 (blend mode, -1 = ids 0x194/0x195), 0x540590 (cull: 1 = the view's
  mode `[[[0x69ed0c]]+4]+0x10`, else 0), 0x540500 (z bias + the view's base
  `+0x30`; deferred into `[0x69e0d4]+0x1180c` as float * [0x5b3e10] when
  `+0x117e4 & 0x20000`), 0x516950 (no-op).
- THRASH_setstate@8 / THRASH_getstate@4 of the driver (the pointers
  [0x6bd97c] / [0x6bd984]) run for real; the D3D device is a logging stub, so
  the rows hold the exact SetRenderState / SetTextureStageState calls.

Synthetic domain (no game data):
- `set` rows: THRASH ids 2, 6, 0xa, 0x18, 0x38, 0x194, 0x195 with values
  -2..11 sent alone (driver caps none / all); `cache` is getstate after.
- `apply` rows: every combination of shade 0..3, blend enable 0..3, blend mode
  -1..3, cull 0..2, z bias {-3, 2}, field 5 -1, on views (cull mode, z base,
  deferred z, caps) (2,0,off,0), (2,0,off,15), (1,4,off,0), (2,4,on,0). Every
  row starts from cache sentinels (0x7fffffff), so each field is applied.
- `seq` rows apply a second block on top of the first (only the changed
  fields are sent).
- `combine` rows send THRASH 0x29 (stage 0) / 0x10029 (stage 1) = 8 and 0;
  the draws send (0x29, 8) and (0x10029, 0) (0x540ce0, 0x5405f0).

Driver caps: .data/.bss as in the file (zero) except the words of the row's
`caps` mask (CAPS_WORDS).

    python scripts/research/replay-render-state.py \
        --out iterations/012-campaign-fidelity/crates/nfs-assets/tests/fixtures/render-state.tsv
"""
import argparse
import itertools
from pathlib import Path
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import render_harness as rh  # noqa: E402
from render_harness import DATA  # noqa: E402

APPLY_LAYER_STATE = 0x540660
CACHE_IDS, CACHE_VALUES, FIELDS = 0x5ded30, 0x5ded34, 6
VIEW_PTR, RENDER_PTR = 0x69ed0c, 0x69e0d4
P, Q, VIEW, RENDER, BLOCK, BLOCK2 = (DATA + 0x100, DATA + 0x200, DATA + 0x300, DATA + 0x1000,
                                     DATA + 0x20000, DATA + 0x20100)
DEFER_FLAGS, DEFERRED_Z = 0x117e4, 0x1180c
DRIVER_SCENE, DRIVER_PENDING = 0x6001822c, 0x60058874
DRIVER_Z = 0x6001885c
# Driver caps words the recorded paths read (zero in the file; set by THRASH_init):
# bit 0 -> [0x6005ab2c] (0x29 = 8: alpha MODULATE instead of SELECTARG1),
# bit 1 -> [0x60058d78] = 1 (0x38 = 1: ONE, ONE instead of SRCALPHA, ONE),
# bit 2 -> [0x6005ab44] (0x6 = 2 accepted: Gouraud + specular),
# bit 3 -> [0x6005a960], [0x6005a990] (per-stage word returned by 0x29).
CAPS_WORDS = ((1, 0x6005ab2c), (2, 0x60058d78), (4, 0x6005ab44), (8, 0x6005a960), (8, 0x6005a990))
SENTINEL = 0x7fffffff


def fmt_f32(x):
    return format(struct.unpack('<I', struct.pack('<f', x))[0], '08x')


class Recorder:
    def __init__(self, game_dir):
        self.m = m = rh.RenderMachine(game_dir)
        self.setstate = m.exports['_THRASH_setstate@8']
        self.getstate = m.exports['_THRASH_getstate@4']
        self.ids = [m.get_u32(CACHE_IDS + 8 * k) for k in range(FIELDS)]
        self.thrash = []
        from unicorn import UC_HOOK_CODE
        from unicorn.x86_const import UC_X86_REG_ESP

        def on_entry(uc, address, _size, _user):
            sp = uc.reg_read(UC_X86_REG_ESP)
            if address == self.setstate:
                ident, value = struct.unpack('<Ii', uc.mem_read(sp + 4, 8))
                self.thrash.append(f'set:{ident:x}={value}')
            else:
                ident, = struct.unpack('<I', uc.mem_read(sp + 4, 4))
                self.thrash.append(f'get:{ident:x}')
        m.uc.hook_add(UC_HOOK_CODE, on_entry, begin=self.setstate, end=self.setstate)
        m.uc.hook_add(UC_HOOK_CODE, on_entry, begin=self.getstate, end=self.getstate)
        self.pristine = {lo: m.get_bytes(lo, hi - lo) for lo, hi in
                         ((0x60017000, 0x600fd000), (0x5de000, 0x5df000))}

    def reset(self, view_cull, z_base, defer, caps=0):
        m = self.m
        for lo, data in self.pristine.items():
            m.put_bytes(lo, data)
        m.put_bytes(DATA, bytes(0x30000))
        m.put_u32(rh.THRASH_SETSTATE_PTR, self.setstate)
        m.put_u32(rh.THRASH_GETSTATE_PTR, self.getstate)
        m.put_u32(rh.DRIVER_DEVICE, m.device)
        m.put_u32(DRIVER_SCENE, 1)
        m.put_u32(DRIVER_PENDING, 0)
        for bit, word in CAPS_WORDS:
            m.put_u32(word, 1 if caps & bit else 0)
        m.put_u32(VIEW_PTR, P)
        m.put_u32(P, Q)
        m.put_u32(Q + 4, VIEW)
        m.put_i32(VIEW + 0x10, view_cull)
        m.put_i32(VIEW + 0x30, z_base)
        m.put_u32(RENDER_PTR, RENDER)
        m.put_u32(RENDER + DEFER_FLAGS, 0x20000 if defer else 0)
        for k in range(FIELDS):
            m.put_i32(CACHE_VALUES + 8 * k, SENTINEL)

    def apply(self, block):
        m = self.m
        m.put_bytes(BLOCK, struct.pack('<6i', *block))
        self.thrash, m.log = [], []
        m.call(APPLY_LAYER_STATE, BLOCK)
        return self.outputs()

    def thrash_set(self, ident, value):
        self.thrash, self.m.log = [], []
        ret = self.m.call(self.setstate, ident, value)
        out = self.outputs()
        out['ret'] = str(ret)
        return out

    def outputs(self):
        m = self.m
        d3d = []
        for name, args in m.log:
            if name == 'SetRenderState':
                d3d.append(f'rs{args[0]}={args[1]}')
            elif name == 'SetTextureStageState':
                d3d.append(f'tss{args[0]}.{args[1]}={args[2]}')
            else:
                d3d.append(name)
        return {
            'thrash': ';'.join(self.thrash) or '-',
            'd3d': ';'.join(d3d) or '-',
            'cache': ','.join(str(m.get_i32(CACHE_VALUES + 8 * k)) for k in range(FIELDS)),
            'driver_z': fmt_f32(m.get_f32(DRIVER_Z)),
            'deferred_z': fmt_f32(m.get_f32(RENDER + DEFERRED_Z)),
        }


def blocks():
    for shade, enable, mode, cull, z in itertools.product(
            range(4), range(4), range(-1, 4), range(3), (-3, 2)):
        yield (shade, enable, mode, cull, z, -1)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--game-dir', type=Path, default=None)
    ap.add_argument('--out', type=Path, required=True)
    args = ap.parse_args()
    rec = Recorder(args.game_dir)
    cols = ['kind', 'view_cull', 'z_base', 'defer', 'caps', 'prev', 'block',
            'thrash', 'd3d', 'cache', 'driver_z', 'deferred_z', 'ret']
    rows = []
    # Single THRASH states as the setters send them (the driver port's whole domain).
    for caps in (0, 15):
        for ident in (2, 6, 0xa, 0x18, 0x38, 0x194, 0x195):
            for value in range(-2, 12):
                rec.reset(2, 0, 0, caps)
                out = rec.thrash_set(ident, value)
                out['cache'] = str(rec.m.call(rec.getstate, ident) & 0xffffffff)
                rows.append(dict(kind='set', view_cull=2, z_base=0, defer=0, caps=caps, prev='-',
                                 block=f'{ident:x}={value}', **out))
    views = [(2, 0, 0, 0), (2, 0, 0, 15), (1, 4, 0, 0), (2, 4, 1, 0)]
    for view_cull, z_base, defer, caps in views:
        for block in blocks():
            rec.reset(view_cull, z_base, defer, caps)
            out = rec.apply(block)
            rows.append(dict(kind='apply', view_cull=view_cull, z_base=z_base, defer=defer, caps=caps,
                             prev='-', block=','.join(map(str, block)), ret='-', **out))
    # Sequences: the four blocks of the shipped tracks (Opaque; MatState / FakeSort; Normal /
    # FullAmbient / UVAnimate; S_Normal / S_UVAnimate) and neighbours, applied in turn.
    shipped = [(1, 0, 0, 1, 0, -1), (1, 2, 0, 0, 0, -1), (1, 2, 0, 1, 0, -1), (1, 2, 0, 1, -3, -1),
               (1, 0, 0, 0, 0, -1),
               (1, 2, 1, 1, 0, -1), (1, 2, -1, 0, 0, -1), (1, 2, 2, 0, -3, -1), (2, 0, 3, 1, 2, -1)]
    for prev, block in itertools.product(shipped, shipped):
        rec.reset(2, 0, 0)
        rec.apply(prev)
        out = rec.apply(block)
        rows.append(dict(kind='seq', view_cull=2, z_base=0, defer=0, caps=0,
                         prev=','.join(map(str, prev)), block=','.join(map(str, block)), ret='-', **out))
    # Single-texture combine of the draws: (0x29, 8) on stage 0, (0x10029, 0) on stage 1.
    for caps in (0, 1, 8, 9):
        for ident, value in ((0x29, 8), (0x10029, 8), (0x29, 0), (0x10029, 0)):
            rec.reset(2, 0, 0, caps)
            out = rec.thrash_set(ident, value)
            rows.append(dict(kind='combine', view_cull=2, z_base=0, defer=0, caps=caps, prev='-',
                             block=f'{ident:x}={value}', **out))
    lines = ['# replay-render-state.py: nfs5.exe 0x540660 + Drivers/dx7/dx7z.dll THRASH_setstate',
             f'# cache ids {",".join(f"{i:x}" for i in rec.ids)}; driver_z = [0x6001885c], '
             'deferred_z = [0x69e0d4]+0x1180c (f32 bits); set rows: cache = getstate after',
             '\t'.join(cols)]
    lines += ['\t'.join(str(r[c]) for c in cols) for r in rows]
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{args.out}: {len(rows)} rows')


if __name__ == '__main__':
    main()

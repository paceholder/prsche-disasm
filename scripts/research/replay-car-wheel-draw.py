#!/usr/bin/env python3
"""Record the Direct3D 7 state in effect at every draw of the car wheels (the `CarWheel` page:
tyre and rim) and the order of the car's part draws, change original-car-materials (the tyre
showing the rim through it, A318).

Executes nfs5.exe and Drivers/dx7/dx7z.dll in Unicorn: the machine of replay-car-draw.py
(render_harness.RenderMachine, PACODE, the real car scene loader 0x44a770 with its render
methods, the recorded light and sky-map states) plus, as the race view draws a car:

- The view's render state object ([[[0x69ed0c]]+4], the harness view of replay-sky-layer.py):
  its constructor 0x537700 (defaults: THRASH 0x28 = 3, 4 = 1, 9, 0x15, 0xe, 0x16, 0x17, 0xf,
  0x3c, 0x39 = 1, 0x43, then the texture stage defaults 0x535b00), the race view's cull
  0x537890(2) (0x468ba0, unmirrored), then 0x468ae0's `0x537870(1)` (THRASH 0x39) and
  `0x537830(3)` (THRASH 0x28) before the scene draws.
- The car's LODs as the LOD builder makes them: `R3DCar_CreateLOD` 0x43adb0 for level 1..n
  (LOD i = level i+1, vector R3DCar+0xb0) and the real `R3DCarLOD` init 0x43c1b0 (vtable slot
  1): article selection by the variant words, the part sort 0x43e710 / 0x43e910 / 0x43ea20,
  part records and their vertex copies.
- The real pass function 0x444e00 (exterior view: driver 0x80, body 0x50, wheels 0x20) ->
  `R3DCarLOD_DrawParts` 0x43c830 (THRASH 0x24 = 10, pass 0 / pass 1 over the parts, `tr`
  matrix, wheel pivot 0x438210, channel rotations 0x443f10, `pr` lookup 0x43cd50) -> binder
  0x44a250 -> method draw 0x53f1b0 and every layer down to THRASH_drawtrimesh -> dx7z.dll ->
  the logging D3D7 device.

Nothing between the state prefix and the car is replayed: the scene draws between 0x468ae0's
z-write set and the car (track 0x467290, 0x455260) and the car shadow 0x43b820 restore
THRASH 0x39 to 1 after their own 0 (read: 0x43b820, 0x459020, 0x459200, 0x45d1a0, 0x47a060).

The logging device is stateful: every SetRenderState / SetTextureStageState / SetTexture of
the whole run (prefix and draws) is applied to a state table, and every DrawIndexedPrimitive
row carries the table as it is then. States the run never set are absent (device defaults).

Harness inputs (not game code): R3DCar fields the passes read: +0xac = the material holder,
+0xb0/+0xb4 the LOD vector, +0x2c0 = 0 (LOD 0), +0x2d0 = channel array (12 bytes per channel:
x, y, z turns; wheel spin per case), +0x6d0 the four wheel positions (the car's `Base` wheel
pivots are not read: the harness puts (+-0.7, 0.3, +-1.1)), +0xf4 = the vehicle (zero but
+0x558 -> zero block); car detail [0x6197c4] = 2; `__RTDynamicCast` 0x5a134b returns its
argument. The `+0x40` vertex copies 0x43c1b0 makes are the undamaged `vt`.

Driver caps: THRASH_init fills them from the D3D device (zero in the file). Without them the
driver refuses THRASH 4 (z buffer), 0x24 (alpha test) and 0x28 (z function) and sends nothing;
the car draws are recorded with z buffer [0x6005aaf4], alpha compare [0x6005aba0] and every
compare function [0x6005abe0 + 4 f] set (any Direct3D 7 HAL device; `map0` rows show the rest).

    python scripts/research/replay-car-wheel-draw.py --game-dir local/game \\
        --map-out iterations/012-campaign-fidelity/crates/nfs-assets/tests/fixtures/thrash-z-alpha.tsv
    python scripts/research/replay-car-wheel-draw.py --game-dir local/game --fixtures local/fixtures \\
        --out local/fixtures/car-wheel-draw.tsv

Rows (tab separated):
- `map1|map0 <THRASH sequence> <return values> <D3D calls>` (--map-out, synthetic): the driver
  alone, fresh, with (map1) and without (map0) the CAPS words.
- `prefix <case> - <D3D calls>`: the view's render state as the race view sets it.
- `part <car>/lod<k> <j> level, article, Base mask / wheel pivot / channel / pass-1 flag`: the
  LOD part records in the order 0x43c1b0 left them (the draw order).
- `bind <case> <n> mask, lod, part, article, pr, material, method, mt flags, count`: every
  0x44a250 call of 0x444e00 in order (the draw order of the `pr` groups).
- `draw <case> <n> src=...;tex;rs;tss;nv;ni;diffuse`: every DrawIndexedPrimitive: its sources
  (`mask:part:article:pr:material:method:method pass`, found by vertex bytes: the driver batches
  submissions of equal state), and the render states, texture stage states and texture in
  effect, the diffuse words of its vertices.
"""
import argparse
import importlib.util
import struct
import sys
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import render_harness as rh  # noqa: E402
import ai_harness as ah  # noqa: E402

_spec = importlib.util.spec_from_file_location('car_draw', HERE / 'replay-car-draw.py')
cd = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cd)
sd, sl = cd.sd, cd.sd.sl

RS_CTOR, RS_CULL, RS_ZWRITE, RS_ZFUNC = 0x537700, 0x537890, 0x537870, 0x537830
CREATE_LOD, LOD_INIT, PASSES, DRAW_PARTS = 0x43adb0, 0x43c1b0, 0x444e00, 0x43c830
RTTI_CAST, BIND, METHOD_PASS = 0x5a134b, 0x44a250, 0x53f620
G_DETAIL, G_SHOWROOM, G_CAM_FLAGS = 0x6197c4, 0x65b928, 0x627f43
CPU_COPY_FLAGS = (0x5deb1c, 0x5deb18, 0x5deb10, 0x5deb30)   # 0x5323e0: the plain copy
WHEELS = [(-0.7, 0.3, 1.1), (0.7, 0.3, 1.1), (-0.7, 0.3, -1.1), (0.7, 0.3, -1.1)]
# THRASH sequences on a fresh driver (synthetic): the z and alpha-test ids the frame and the car
# path send outside the material block (4, 0x28, 0x39, 0x24, 0x25) over their value ranges, and
# the material ids the wheel draws use (2, 6, 0xa, 0x38).
MAP_SEQS = ([[(0x24, v)] for v in (0, 1, 10, 128, 255)] + [[(0x25, v)] for v in range(9)]
            + [[(0x25, 7), (0x24, 10)], [(0x24, 10), (0x24, 0)]]
            + [[(0x28, v)] for v in range(9)] + [[(4, v)] for v in range(3)] + [[(0x39, v)] for v in (0, 1)]
            + [[(0x28, 1), (4, 1)], [(0x28, 1), (4, 0)], [(4, 0), (4, 1)], [(0x28, 6), (0x28, 3), (4, 1)]]
            + [[(2, v)] for v in range(3)] + [[(6, 1)], [(0xa, 0)], [(0xa, 2)], [(0x38, 0)]])
# The same without the caps: what a device without z buffer / alpha compare gets.
MAP_SEQS_NO_CAPS = [[(0x24, 10)], [(0x24, 0)], [(0x28, 3)], [(4, 1)], [(0x39, 1)]]
# Eye offsets in the car frame (lateral, up, forward), looking at (0, 0.6, 0): tail camera,
# the playtest view of the rear wheel from behind-right (docs/ui-reference
# car-tyre-see-through-rim-90.png), a low side view of the front wheel.
EYES = [(0.0, 1.9, -6.5), (2.6, 0.7, -3.2), (2.2, 0.4, 1.4)]
SPINS = [0.0, 0.3]
CARS = ('356a', '993')


class WheelDraw(cd.CarDraw):
    def __init__(self, game_dir):
        super().__init__(game_dir)
        m = self.m
        m.stubs[RTTI_CAST] = ('rtti', 0, lambda m_, _a: self.args(1)[0])
        self.state = None
        self.rows_dip = None
        self.where = {}
        self.vmap = {}
        for va, (name, pop, h) in list(m.stubs.items()):
            if name in ('SetRenderState', 'SetTextureStageState', 'SetTexture', 'DrawIndexedPrimitive'):
                def wrap(m_, args, h=h, name=name):
                    r = h(m_, args)
                    self.track(name, args)
                    return r
                m.stubs[va] = (name, pop, wrap)

    def track(self, name, args):
        if self.state is None:
            return
        rs, tss, tex = self.state
        a = args[1:]
        if name == 'SetRenderState':
            rs[a[0]] = a[1]
        elif name == 'SetTextureStageState':
            tss[(a[0], a[1])] = a[2]
        elif name == 'SetTexture':
            tex[a[0]] = a[1]
        elif name == 'DrawIndexedPrimitive' and self.rows_dip is not None:
            ptype, fvf, vptr, nv, iptr, ni = a[:6]
            vb = self.m.get_bytes(vptr, 32 * nv)
            diffuse = [struct.unpack_from('<I', vb, 32 * i + 16)[0] for i in range(nv)]
            # The driver batches submissions of equal state: name the sources of the drawn
            # vertices by their bytes (rhw, colours, uv; x, y, z are mapped to the screen).
            src = []
            for i in range(nv):
                tag = self.vmap.get(vkey(vb[32 * i:32 * i + 32]), '?')
                if tag not in src:
                    src.append(tag)
            self.rows_dip.append(dict(src=src, ptype=ptype, fvf=fvf, nv=nv, ni=ni,
                                      rs=dict(rs), tss=dict(tss), tex=dict(tex), diffuse=diffuse))

    # -- car object, LODs --------------------------------------------------------
    def build_car(self, data):
        m = self.m
        g = m.get_u32
        for va in CPU_COPY_FLAGS:
            m.put_u32(va, 0)
        m.put_u32(G_DETAIL, 2)
        m.put_u32(G_SHOWROOM, 0)
        m.put_bytes(G_CAM_FLAGS, b'\0')
        r3d = self.r3d
        veh = self.alloc(0x1000)
        m.put_u32(veh + 0x558, self.alloc(0x600))
        m.put_u32(veh + 0x520, 0xffffffff)        # not the viewed car (0x465d80)
        m.put_u32(r3d + 0xf4, veh)
        m.put_u32(r3d + 0xac, self.holder)
        self.channels = self.alloc(12 * 0x20)
        m.put_u32(r3d + 0x2d0, self.channels)
        for i, p in enumerate(WHEELS):
            m.put_bytes(r3d + 0x6d0 + 12 * i, struct.pack('<3f', *p))
        m.put_u32(r3d + 0x2c0, 0)
        m.put_bytes(r3d + 0x471, b'\0')
        levels = sorted({lv for _a, lv in cd_levels(data)})
        lods = []
        for lv in range(1, max(levels) + 1):
            lod = m.call(CREATE_LOD, lv, ecx=r3d)
            m.call(LOD_INIT, CRP_ROOT, ecx=lod, budget=200_000_000)
            lods.append(lod)
        vec = self.alloc(4 * len(lods))
        for k, lod in enumerate(lods):
            m.put_u32(vec + 4 * k, lod)
        m.put_u32(r3d + 0xb0, vec)
        m.put_u32(r3d + 0xb4, vec + 4 * len(lods))
        self.lods = lods
        self.finish_textures()
        return [(k, g(lod + 8), g(lod + 0xc), g(lod + 0x10)) for k, lod in enumerate(lods)]

    def set_spin(self, spin):
        for ch in range(1, 5):
            self.m.put_bytes(self.channels + 12 * ch, struct.pack('<3f', -spin, 0.0, 0.0))

    def state_prefix(self):
        """The view's render state object as the race view leaves it before the cars."""
        m = self.m
        set_caps(m)
        view = sl.VIEW
        m.call(RS_CTOR, ecx=view)
        m.call(RS_CULL, 2, ecx=view)
        m.call(RS_ZWRITE, 1, ecx=view)
        m.call(RS_ZFUNC, 3, ecx=view)


CRP_ROOT = cd.CRP


def vkey(v):
    """Vertex identity: rhw, diffuse, specular, u, v (the driver maps x, y, z to the screen)."""
    return v[12:32]


# Driver caps words THRASH_init fills from the D3D7 device (zero in the file): z buffer
# [0x6005aaf4] (THRASH 4), alpha compare [0x6005aba0] (THRASH 0x24), compare functions
# [0x6005abe0 + 4 * func] (THRASH 0x25 / 0x28). Set: a device with a z buffer and every
# compare function (the cases the driver refuses otherwise send nothing).
CAPS = [0x6005aaf4, 0x6005aba0] + [0x6005abe0 + 4 * k for k in range(8)]


def set_caps(m):
    for va in CAPS:
        m.put_u32(va, 1)


def cd_levels(data):
    """(article index, level) of every Base level entry (as replay-car-deform.py)."""
    arts, _mts = cd.articles(data)
    out = []
    for ai, a in enumerate(arts):
        base = next((k for k in a['kids'] if k['tag'] == b'Base'), None)
        if base is None:
            continue
        o = base['data']
        first, last = struct.unpack_from('<II', data, o + 8)
        for k in range(first, last):
            out.append((ai, struct.unpack_from('<H', data, o + 0x64 + 12 * k + 8)[0]))
    return out


def fmt_map(d):
    return ','.join(f'{k}={v}' for k, v in sorted(d.items()))


def record_map(game_dir):
    """`map1|map0 <sequence> <results> <D3D calls>`: THRASH_setstate@8 of dx7z.dll on a fresh
    driver with (map1) and without (map0) CAPS; results are the driver's return values."""
    rows = []
    for caps, seqs in ((1, MAP_SEQS), (0, MAP_SEQS_NO_CAPS)):
        for seq in seqs:
            rec = sd.DrawRecorder(game_dir)
            rec.reset()
            rec.setup_draw()
            if caps:
                set_caps(rec.m)
            rec.m.log.clear()
            results = [rec.m.call(rec.setstate, tid, val & 0xffffffff) for tid, val in seq]
            rec.m.call(sl.DRIVER_FLUSH)
            calls = [sd.event_text((n, a)) for n, a in rec.m.log
                     if n in ('SetRenderState', 'SetTextureStageState')]
            rows.append((f'map{caps}', ','.join(f'{tid:x}={val}' for tid, val in seq), ','.join(map(str, results)),
                         '|'.join(calls) or '-'))
    return rows


def record(game_dir, fixtures):
    light = cd.read_rows(fixtures / 'car-light-tracks.tsv', 'place')
    env = cd.read_rows(fixtures / 'car-env-map-tracks.tsv', 'update')
    rows = []
    track, key = 'forest', '700,0.0,0.0,0.0'
    for car in CARS:
        path = next(p for p in (game_dir / 'GameData/CarModel').iterdir() if p.name.lower() == f'{car}.crp')
        data = path.read_bytes()
        if data[:4] != b' raC':
            data = ah.refpack(data)
        arts, mts = cd.articles(data)
        by_header = {CRP_ROOT + a['entry']['off']: a for a in arts}
        prs = {}
        for a in arts:
            for k in a['kids']:
                if k['tag'] == b'pr':
                    prs[CRP_ROOT + k['data']] = (a['name'], k['idx'])
        rec = WheelDraw(game_dir)
        rec.load_car(data)
        lods = rec.build_car(data)
        g = rec.m.get_u32
        for k, lv, parts, n in lods:
            for j in range(n):
                p = parts + 0x54 * j
                a = by_header.get(g(p))
                info = rec.m.get_bytes(g(p + 0x44), 16)
                rows.append(('part', f'{car}/lod{k}', j, f'level={lv};article={a["name"] if a else "?"};'
                             f'mask={info[10]:x};pivot={struct.unpack("b", info[4:5])[0]};channel={info[7]};'
                             f'pass1={rec.m.get_bytes(g(p + 0x4c), 1)[0] & 1}'))
        lw, ev = light[(track, key)], env[(track, key)]
        rec.set_light(dict(state=[int(w, 16) for w in lw['state'].split(',')], mirror=int(lw['mirror'])), ev)
        pos = [cd.unbits(h) for h in ev['pos'].split(',')]
        fr = [cd.unbits(h) for h in ev['car_frame'].split(',')]
        mat_name, mat_flags = {}, {}
        for idx, mt in mts.items():
            mat_name[idx] = data[mt['data'] + 16:mt['data'] + 32].split(b'\0')[0].decode('latin-1')
            mat_flags[idx] = struct.unpack_from('<I', data, mt['data'] + 12)[0]
        for e, eye_off in enumerate(EYES):
            for s, spin in enumerate(SPINS):
                lat, up, fwd = fr[0:3], fr[3:6], fr[6:9]
                eye = [pos[i] + eye_off[0] * lat[i] + eye_off[1] * up[i] + eye_off[2] * fwd[i] for i in range(3)]
                target = [pos[i] + 0.6 * up[i] for i in range(3)]
                rec.set_camera(eye, target)
                rec.set_model(fr, pos)
                rec.set_spin(spin)
                cid = f'{car}/{track}/700/{e}/{s}'
                # Fresh driver state and THRASH caches per case, as at the start of a frame.
                for k in range(6):
                    rec.m.put_i32(sl.CACHE_VALUES + 8 * k, 0x7fffffff)
                for k in range(14):
                    rec.m.put_i32(0x5de6c0 + 8 * k, 0x7fffffff)
                rec.state = ({}, {}, {})
                rec.m.log.clear()
                rec.state_prefix()
                rec.m.call(sl.DRIVER_FLUSH)
                prefix = [sd.event_text((n, a)) for n, a in rec.m.log
                          if n in ('SetRenderState', 'SetTextureStageState')]
                rows.append(('prefix', cid, '-', '|'.join(prefix)))
                rec.rows_dip = []
                rec.vmap = {}
                rec.where = dict(mask='-', lod=-1, part=-1, article='-', pr='-', material='-', mpass=-1)
                binds = []
                hooks = []

                def on_parts(uc, _a, _s, _u):
                    # 0x43c830 (thiscall on the LOD: draw context, mask, in-car flag).
                    sp = uc.reg_read(UC_X86_REG_ESP)
                    mask = struct.unpack('<I', uc.mem_read(sp + 8, 4))[0] & 0xff
                    lod = uc.reg_read(UC_X86_REG_ECX)
                    rec.where.update(mask=f'{mask:x}', lod=rec.lods.index(lod), parts=g(lod + 0xc), mpass=-1)

                def on_bind(uc, _a, _s, _u):
                    # 0x44a250 (thiscall on the material record: count, part record, `pr` data).
                    sp = uc.reg_read(UC_X86_REG_ESP)
                    count, part, grp = struct.unpack('<III', uc.mem_read(sp + 4, 12))
                    name, idx = prs.get(grp, ('?', -1))
                    mat = g(grp + 4) & 0xffff
                    w = rec.where
                    w.update(part=(part - w['parts']) // 0x54, article=name, pr=idx, material=mat, count=count,
                             method=mat_name.get(mat, '?'), mpass=-1)
                    binds.append(f'mask={w["mask"]};lod={w["lod"]};part={w["part"]};article={name};pr={idx};'
                                 f'material={mat};method={w["method"]};mt_flags={mat_flags.get(mat, 0):x};'
                                 f'count={count}')

                def on_pass(uc, _a, _s, _u):
                    w = rec.where
                    w['mpass'] += 1
                    tag = (f'{w["mask"]}:{w["part"]}:{w["article"]}:{w["pr"]}:{w["material"]}:{w["method"]}:'
                           f'{w["mpass"]}')
                    slots = rec.m.get_bytes(g(0x69e0d4) + 0x434, 32 * w['count'])
                    for i in range(w['count']):
                        rec.vmap[vkey(slots[32 * i:32 * i + 32])] = tag

                hooks.append(rec.m.uc.hook_add(UC_HOOK_CODE, on_parts, begin=DRAW_PARTS, end=DRAW_PARTS))
                hooks.append(rec.m.uc.hook_add(UC_HOOK_CODE, on_bind, begin=BIND, end=BIND))
                hooks.append(rec.m.uc.hook_add(UC_HOOK_CODE, on_pass, begin=METHOD_PASS, end=METHOD_PASS))
                try:
                    rec.events = []
                    rec.m.call(PASSES, 0, ecx=rec.r3d, budget=4_000_000_000)
                    rec.m.call(sl.DRIVER_FLUSH)
                finally:
                    rec.events = None
                    for h in hooks:
                        rec.m.uc.hook_del(h)
                rows += [('bind', cid, n, b) for n, b in enumerate(binds)]
                n_wheel = 0
                for n, d in enumerate(rec.rows_dip):
                    rows.append(('draw', cid, n, f'src={"|".join(d["src"])};'
                                 f'tex={fmt_map(d["tex"])};rs={fmt_map(d["rs"])};'
                                 f'tss={",".join(f"{a}.{b}={v}" for (a, b), v in sorted(d["tss"].items()))};'
                                 f'nv={d["nv"]};ni={d["ni"]};diffuse={",".join(f"{w:08x}" for w in d["diffuse"])}'))
                    n_wheel += any(':CarWheel:' in t for t in d['src'])
                rec.rows_dip = None
                rec.state = None
                print(f'{cid}: {len(rows)} rows, {n_wheel} wheel draws', flush=True)
    return rows


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--game-dir', type=Path, default=rh.ROOT / 'local/game')
    ap.add_argument('--fixtures', type=Path, default=rh.ROOT / 'local/fixtures',
                    help='directory with car-light-tracks.tsv and car-env-map-tracks.tsv')
    ap.add_argument('--out', type=Path, help='car draws (game data: local/fixtures)')
    ap.add_argument('--map-out', type=Path, help='THRASH -> D3D7 sequences (synthetic, no game data)')
    args = ap.parse_args()
    if args.map_out:
        write(args.map_out, record_map(args.game_dir),
              '# replay-car-wheel-draw.py --map-out: dx7z.dll THRASH_setstate@8 0x60003ca0 on a fresh driver '
              '(caps: z buffer, alpha compare, every compare function) -> D3D7 SetRenderState; synthetic inputs')
    if args.out:
        write(args.out, record(args.game_dir, args.fixtures),
              '# replay-car-wheel-draw.py: nfs5.exe 0x537700/0x537890/0x537870/0x537830 view state, '
              '0x43adb0 + 0x43c1b0 LODs, 0x444e00 -> 0x43c830 -> 0x44a250 + 0x53f1b0 -> dx7z.dll -> D3D7 '
              'state at every DrawIndexedPrimitive; 356A / 993 (game data: keep in local/fixtures)')


def write(path, rows, head):
    lines = [head, 'kind\tcase\tkey\tout'] + ['\t'.join(str(c) for c in r) for r in rows]
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{path}: {len(rows)} rows')


if __name__ == '__main__':
    main()

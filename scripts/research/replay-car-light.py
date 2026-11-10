#!/usr/bin/env python3
"""Record how the original lights a car (pass 1 of every car render method, change
original-lighting; docs/re/lighting.md "Car vertex lighting" in the workspace root).

Executes nfs5.exe in Unicorn (the machine of replay-car-env-map.py: vehicle_harness.py with
the real support tree of replay-road-support.py, x87 control word 0x27f):

- `colour` rows: `Road_SampleColour` 0x4746a0 (thiscall on a locator copy: out, point) ->
  the probe 0x474060(point, 1) on the real support tree -> 0x474700 (the polygon's RGB555
  vertex colours as heights of three planes 0x474610 / 0x48a060, clamped to 1). Recorded:
  the polygon found (insertion index, -1 none) and the four floats (1, r, g, b).
- `light` rows: the lighting part of `R3DCar_UpdateLighting` 0x43a560 (asm 0x43a8a6 up to
  0x43ad59, after the sky-map part of replay-car-env-map.py): the road colour S of the car
  (0x4746a0 on the car's locator, as above), the `<track>.map [header]` colours of the
  track lighting object [0x6115d8] (+0 tint, +0x10 ambient, +0x20 interior, +0x30
  interiordiffuse, +0x40 lightdecal, a,r,g,b), the covered flag (`0x473330` & 0xc000,
  byte [esp+0x48]), the sun of the sky object [0x627e5c]+0x11c, the specular options
  [0x657424] / [0x657d64] and the camera type [[0x628130]+0x10]+0x2c. Recorded: the light
  state words +0x18..+0xd0 (ambient slots 0..2, light direction, diffuse slots 0..2,
  specular flags, light and specular colours, power) and the mirror byte +0x190.
  `--tracks` runs the whole 0x43a560 (entry to 0x43ad59) on the env recorder's placements
  (real `.map`, real support polygons with their colours, real node graph; the sun is the
  real `Sky_SunPosition` 0x461090 on the track's `Sky/<track>.bin` fields, as the flare
  object 0x460780 stores it at +0x11c).
- `vertex` rows: `R3DLightLayer::Apply` 0x446530 (vtable 0x5b393c slot 0, thiscall: layer
  parameter block {kA, kD, slot}, output) on a draw context [0x69e0d4] as replay-car-env-map.py
  builds it (vertex count +4, polygon size +0x20, flags +0x117e4, skip words +0x11000, the
  vertex array +0x434 with colour +0x10, model and view matrices of [[ctx]+0x5c] / [[ctx]]),
  the light state [0x6115cc]; dependencies: normals, the second channel (its size is the
  vertex count of the cached indexed path), the optional byte index list. Recorded: the
  colour dword written to every vertex (or `-` when untouched), the flags word, the return.

Harness services as replay-car-env-map.py (heap, INI reader, texture registry, road colour
query 0x456160 of the lamp list returns no lamp); nothing else is replaced.

    python scripts/research/replay-car-light.py --synthetic \\
        --out iterations/012-campaign-fidelity/crates/nfs-assets/tests/fixtures/car-light.tsv
    python scripts/research/replay-car-light.py --game-dir local/game --tracks \\
        --polygons local/derived/support --out local/fixtures/car-light-tracks.tsv
"""
import argparse
import importlib.util
import math
from pathlib import Path
import random
import struct
import sys

from unicorn.x86_const import (UC_X86_REG_EBP, UC_X86_REG_EBX, UC_X86_REG_EIP, UC_X86_REG_ESP,
                               UC_X86_REG_FPCW)

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import vehicle_harness as vh  # noqa: E402
import ai_harness as ah  # noqa: E402

_spec = importlib.util.spec_from_file_location('carenv', HERE / 'replay-car-env-map.py')
ce = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ce)
rs = ce.rs
f32 = ce.f32
bits = ce.bits

ROAD_COLOUR, LIGHT_TAIL, LIGHT_END, LIGHT_LAYER = 0x4746a0, 0x43a8a6, 0x43ad59, 0x446530
SKY_PTR, SPEC_OPTION, SPEC_DETAIL, RENDER_OBJ_PTR = 0x627e5c, 0x657424, 0x657d64, 0x628130
SUN_POSITION = 0x461090
TRACK_LIGHTING_PTR, LIGHT_PTR, DRAW_CTX_PTR = 0x6115d8, 0x6115cc, 0x69e0d4


def words(m, a, n):
    return ','.join(f'{w:08x}' for w in struct.unpack(f'<{n}I', m.get_bytes(a, 4 * n)))


class LightRecorder(ce.EnvRecorder):
    def __init__(self, game_dir):
        super().__init__(game_dir)
        m = self.m
        self.sky = self.alloc(0x200)
        self.camera = self.alloc(0x40)
        self.owner = self.alloc(0x40)
        self.point = self.alloc(0x10)
        self.out = self.alloc(0x10)
        self.light_globals(0, 0, 0)

    def light_globals(self, spec, detail, cam_type):
        m = self.m
        m.put_u32(SKY_PTR, self.sky)
        m.put_u32(SPEC_OPTION, spec)
        m.put_u32(SPEC_DETAIL, detail)
        m.put_u32(self.render_obj + 0x10, self.camera)
        m.put_u32(self.camera + 0x2c, cam_type)

    # -- road colour ------------------------------------------------------------------
    def road_colour(self, point):
        m = self.m
        m.put_bytes(self.owner, bytes(0x30))
        m.put(self.point, *point)
        m.put_bytes(self.out, bytes(16))
        m.call_thiscall(ROAD_COLOUR, self.owner, self.out, self.point, budget=20_000_000)
        return words(m, self.out, 4)

    def polygon_of(self, point):
        """The polygon 0x474060(point, 0) selects (insertion index, -1 none)."""
        m = self.m
        m.put_bytes(self.owner, bytes(0x30))
        m.put(self.point, *point)
        m.call_thiscall(rs.SELECT, self.owner, self.point, 0, budget=20_000_000)
        p = m.get_u32(self.owner + 0x28)
        return (p - self.poly_base) // rs.POLY_STRIDE if p else -1

    # -- light tail -------------------------------------------------------------------
    def light_tail(self, tl, pos, covered):
        """0x43a8a6..0x43ad59 with the frame of 0x43a560: [esp+0x40] = car position pointer,
        [esp+0x48] = covered byte, [esp+0x4c] = the car's locator (zeroed: the probe selects
        anew), ebx = light state, ebp = light state + 0x18."""
        m = self.m
        ls = self.r3d + ce.LIGHT_OFFSET
        m.put_u32(TRACK_LIGHTING_PTR, tl)
        m.put(self.point, *pos)
        m.put_bytes(self.owner, bytes(0x30))
        # The constants the head of 0x43a560 stores before the sky-map part (asm
        # 0x43a5ac..0x43a6c6): light colour (1,1,1,1) at +0x8c, specular colours (1,.2,.2,.2),
        # (1,.8,.8,.8), (1,1,1,1) at +0x9c / +0xac / +0xbc, power 5 at +0xcc.
        m.put(ls + 0x8c, 1.0, 1.0, 1.0, 1.0)
        m.put(ls + 0x9c, 1.0, f32(0.2), f32(0.2), f32(0.2), 1.0, f32(0.8), f32(0.8), f32(0.8), 1.0, 1.0, 1.0, 1.0, 5.0)
        sp = vh.STACK_TOP - 0x2000
        m.uc.mem_write(sp, bytes(0x120))
        m.put_u32(sp + 0x40, self.point)
        m.put_bytes(sp + 0x48, bytes([1 if covered else 0]))
        m.put_u32(sp + 0x4c, self.owner)
        m.uc.reg_write(UC_X86_REG_ESP, sp)
        m.uc.reg_write(UC_X86_REG_EBX, ls)
        m.uc.reg_write(UC_X86_REG_EBP, ls + 0x18)
        m.uc.reg_write(UC_X86_REG_FPCW, m.fpcw)
        m.uc.emu_start(LIGHT_TAIL, LIGHT_END, count=20_000_000)
        if m.uc.reg_read(UC_X86_REG_EIP) != LIGHT_END:
            raise RuntimeError('0x43a8a6 did not reach 0x43ad59')
        return ls

    def light_full(self):
        """0x43a560 from its entry to 0x43ad59 (the env recorder's car and track)."""
        m = self.m
        m.put_u32(self.r3d + 0xf4, rs.CAR)
        m.put_u32(TRACK_LIGHTING_PTR, self.tl)
        self.globals()
        sp = vh.STACK_TOP - 0x1000
        m.put_u32(sp, vh.EXIT)
        m.uc.reg_write(UC_X86_REG_ESP, sp)
        m.uc.reg_write(ce.UC_X86_REG_ECX, self.r3d)
        m.uc.reg_write(UC_X86_REG_FPCW, m.fpcw)
        self.roof = None
        self.in_run = True
        try:
            m.uc.emu_start(ce.UPDATE_LIGHTING, LIGHT_END, count=40_000_000)
        finally:
            self.in_run = False
        if m.uc.reg_read(UC_X86_REG_EIP) != LIGHT_END:
            raise RuntimeError('0x43a560 did not reach 0x43ad59')
        return self.r3d + ce.LIGHT_OFFSET

    def state_text(self, ls):
        m = self.m
        return (f'state={words(m, ls + 0x18, 0xb8 // 4)};mirror={m.get_bytes(ls + 0x190, 1)[0]}')

    # -- light layer ------------------------------------------------------------------
    def layer(self, ls, case):
        """0x446530 on a synthetic draw context (see the module doc)."""
        m = self.m
        normals = case['normals']
        n = len(normals)
        index = case.get('index')
        count = len(index) if index is not None else n
        ctx = self.ctx
        m.put_bytes(ctx, bytes(ce.CTX_SIZE))
        m.put_u32(ctx, self.ctx_obj)
        view = self.alloc(0x40)
        m.put(view, *case.get('view', [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]))
        m.put_u32(self.ctx_obj + 4, view)
        m.put_u32(self.ctx_obj + 0x60, self.model)
        m.put(self.model, *case['model'])
        m.put_u32(ctx + 4, count)
        m.put_u32(ctx + 0x20, case['poly'])
        m.put_u32(ctx + 0x117e4, case.get('flags', 0))
        for k in case.get('skip', ()):
            m.put_u32(ctx + 0x11000 + 8 * k, 1)
        stride = case.get('stride', 0x20)
        layer = self.alloc(0x30)
        m.put_u32(layer + 8, 0x7e1a0020)
        m.put_u32(layer + 0x14, stride)
        deps = self.alloc(12)
        m.put_u32(layer + 0x28, deps)
        m.put_u32(layer + 0x2c, 3)

        def channel(data, size):
            d, owner, arr = self.alloc(0x24), self.alloc(0x10), self.alloc(0x10)
            buf = self.alloc(max(len(data), 4))
            m.put_bytes(buf, data)
            m.put_u32(d + 0x1c, 0)
            m.put_u32(d + 0x20, owner)
            m.put_u32(owner + 8, arr)
            m.put_u32(arr, size)
            m.put_u32(arr + 4, buf)
            return d
        nb = b''.join(struct.pack('<4f', *v, 0.0) for v in normals)
        m.put_u32(deps, channel(nb, len(nb)))
        m.put_u32(deps + 4, channel(bytes(16 * n), 16 * n))
        m.put_u32(deps + 8, channel(bytes(index), len(index)) if index is not None else 0)
        param, value = self.alloc(8), self.alloc(12)
        ka, kd, slot = case['param']
        m.put(value, ka, kd)
        m.put_u32(value + 8, slot)          # the slot is an integer word (asm 0x4465d8)
        m.put_u32(param + 4, value)
        m.put_u32(LIGHT_PTR, ls)
        m.put_u32(DRAW_CTX_PTR, ctx)
        sentinel = 0x7fc0dead
        for i in range(count):
            m.put_u32(ctx + 0x434 + i * stride + 0x10, sentinel)
        ret = m.call_thiscall(LIGHT_LAYER, layer, param, self.alloc(8), budget=5_000_000)
        out = []
        for i in range(count):
            w = m.get_u32(ctx + 0x434 + i * stride + 0x10)
            out.append('-' if w == sentinel else f'{w:08x}')
        return ('next' if ret == 0x7e1a0020 else f'{ret:08x}'), m.get_u32(ctx + 0x117e4), out


def vertex_text(case):
    idx = case.get('index')
    ka, kd, slot = case['param']
    return (f'param={bits(ka)},{bits(kd)},{slot};poly={case["poly"]};'
            f'flags={case.get("flags", 0):x};stride={case.get("stride", 0x20):x};'
            f'skip={",".join(map(str, case.get("skip", ()))) or "-"};'
            f'index={",".join(map(str, idx)) if idx is not None else "-"};'
            f'model={",".join(bits(v) for v in case["model"])};'
            f'view={",".join(bits(v) for v in case.get("view", [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]))};'
            f'normals={"|".join(",".join(bits(c) for c in v) for v in case["normals"])}')


def vertex_rows(rec, ls, cid, cases):
    rows = []
    for j, case in enumerate(cases):
        ret, flags, out = rec.layer(ls, case)
        rows.append(('vertex', cid, j, vertex_text(case) + f';ret={ret};flags_out={flags:x};'
                     f'colour={"|".join(out)}'))
    return rows


def unit(rng):
    v = [rng.gauss(0, 1) for _ in range(3)]
    ln = math.sqrt(sum(c * c for c in v)) or 1.0
    return tuple(f32(c / ln) for c in v)


def rotation(rng):
    a, b = rng.uniform(0, 2 * math.pi), rng.uniform(-0.3, 0.3)
    ca, sa, cb, sb = math.cos(a), math.sin(a), math.cos(b), math.sin(b)
    r = (ca, 0.0, -sa)
    u = (sa * sb, cb, ca * sb)
    f = (sa * cb, -sb, ca * cb)
    return r, u, f


def layer_cases(rng, model):
    normals = [unit(rng) for _ in range(18)]
    base = dict(normals=normals, model=model, poly=3)
    idx = [0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7, 8, 9, 10, 17, 16, 15, 0, 3, 7]
    out = []
    for p in ((1.0, 1.0, 0), (1.0, 1.0, 1), (1.0, 1.0, 2), (0.5, 1.5, 0), (1.0, 0.0, 0),
              (2.0, 3.0, 1), (1.0, -0.5, 0), (0.0, 1.0, 0)):
        out.append(dict(base, param=(f32(p[0]), f32(p[1]), p[2])))
    out.append(dict(base, param=(1.0, 1.0, 0), skip=(1, 3)))
    out.append(dict(base, param=(1.0, 1.0, 0), flags=0x100))
    out.append(dict(base, param=(1.0, 1.0, 0), poly=1))
    out.append(dict(base, param=(1.0, 1.0, 0), flags=0x200))
    out.append(dict(base, param=(1.0, 1.0, 0), stride=0x24))
    out.append(dict(base, param=(1.0, 1.0, 0), index=idx))
    out.append(dict(base, param=(1.0, 1.0, 1), index=idx, skip=(2,)))
    out.append(dict(base, param=(1.0, 1.0, 0), index=idx, flags=0x100))
    out.append(dict(base, param=(1.0, 1.0, 0), index=idx, flags=0x100, poly=1))
    return out


def synthetic_header(rng, k):
    """Track lighting header: tint, ambient, interior, interiordiffuse, lightdecal (a,r,g,b)."""
    if k == 0:
        return [1, 1, 1, 1, 1, 0.3, 0.3, 0.3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]   # the defaults
    out = []
    for _ in range(5):
        out += [1.0] + [round(rng.uniform(0.0, 1.6), 3) for _ in range(3)]
    return [f32(v) for v in out]


def synthetic_polygons(rng):
    """A 4 x 4 grid of quads (some split into triangles) with random RGB555 colours."""
    polys = []
    for i in range(4):
        for j in range(4):
            x0, z0 = 20.0 * i, 20.0 * j
            y = [f32(rng.uniform(-1, 1)) for _ in range(4)]
            p = [(f32(x0), y[0], f32(z0)), (f32(x0), y[1], f32(z0 + 20)), (f32(x0 + 20), y[2], f32(z0 + 20)),
                 (f32(x0 + 20), y[3], f32(z0))]
            cols = [rng.randrange(0x8000) for _ in range(4)]
            if (i + j) % 3 == 0:
                polys.append(([p[0], p[1], p[2]], 3, cols[:3]))
                polys.append(([p[0], p[2], p[3]], 3, [cols[0], cols[2], cols[3]]))
            else:
                polys.append((p, 3, cols))
    # Saturated colours (0x7fff) and black on one more quad.
    polys.append(([(f32(100), 0.0, f32(0)), (f32(100), 0.0, f32(20)), (f32(120), 0.0, f32(20)),
                   (f32(120), 0.0, f32(0))], 3, [0x7fff, 0x0000, 0x7c00, 0x03e0]))
    return polys


def synthetic(game_dir):
    rec = LightRecorder(game_dir)
    rng = random.Random(11)
    polys = synthetic_polygons(rng)
    rec.build_support(polys)
    rows = []
    for k, (pts, flags, cols) in enumerate(polys):
        rows.append(('polygon', 'grid', k, f'vertices={"|".join(",".join(bits(c) for c in p) for p in pts)};'
                     f'flags={flags};colours={",".join(f"{c:04x}" for c in cols)}'))
    points = [(f32(rng.uniform(-5, 125)), f32(rng.uniform(-1, 3)), f32(rng.uniform(-5, 85))) for _ in range(60)]
    points += [(f32(10.0), 0.0, f32(10.0)), (f32(110.0), 0.0, f32(10.0)), (f32(100.0), 0.0, f32(0.0)),
               (f32(500.0), 0.0, f32(500.0))]
    for j, p in enumerate(points):
        rows.append(('colour', 'grid', j, f'point={",".join(bits(c) for c in p)};poly={rec.polygon_of(p)};'
                     f'out={rec.road_colour(p)}'))
    tl = rec.alloc(0x60)
    for k in range(6):
        header = synthetic_header(rng, k)
        rec.m.put(tl, *header)
        sun = [f32(rng.uniform(-200, 200)), f32(rng.uniform(-50, 300)), f32(rng.uniform(-200, 200)), 1.0]
        if k == 5:
            sun = [0.0, f32(150.0), 0.0, 1.0]
        for spec, detail, cam in ((0, 0, 0), (2, 0, 0), (0, 1, 1)):
            rec.light_globals(spec, detail, cam)
            rec.m.put(rec.sky + 0x11c, *sun)
            for covered in (False, True):
                p = points[(k * 7 + spec + 3 * covered) % len(points)]
                ls = rec.light_tail(tl, p, covered)
                if (spec, covered) == (0, False):
                    open_state = rec.m.get_bytes(ls, 0x200)
                rows.append(('light', f'h{k}', f'{spec},{detail},{cam},{int(covered)}',
                             f'header={",".join(bits(v) for v in header)};sun={",".join(bits(v) for v in sun)};'
                             f'point={",".join(bits(c) for c in p)};S={rec.road_colour(p)};' + rec.state_text(ls)))
        r, u, f = rotation(rng)
        model = [f32(v) for v in ce.model_matrix((r, u, f), (rng.uniform(-50, 50), 0.3, rng.uniform(-50, 50)))]
        cases = layer_cases(rng, model)
        rows += vertex_rows(rec, ls, f'h{k}/covered', cases)
        rec.m.put_bytes(ls, open_state)
        rows.append(('light', f'h{k}/open', '-', rec.state_text(ls)))
        rows += vertex_rows(rec, ls, f'h{k}/open', cases)
    return rows, 'synthetic inputs (no game data)'


# -- tracks --------------------------------------------------------------------------------
PLACEMENTS = {
    'forest': [(700, 0.0, 0.0, 0.0), (700, 2.5, 1.0, 15.0), (701, 1.0, 0.5, 90.0), (300, 0.0, 0.0, 0.0),
               (5, 0.0, 0.0, 0.0)],
    'coastal': [(190, 0.0, 0.0, 0.0), (190, 2.5, 1.0, 10.0), (193, -1.5, 0.0, -10.0), (198, 0.0, 2.0, 0.0),
                (0, 0.0, 0.0, 0.0), (100, 0.0, 0.0, 0.0), (400, 0.0, 0.0, 0.0)],
    'farmland': [(70, 0.0, 0.0, 0.0), (300, 1.0, 0.0, 0.0)],
}


def find(dirpath, name):
    for p in dirpath.iterdir():
        if p.name.lower() == name.lower():
            return p
    return None


def track_sun(rec, game_dir, track):
    """`Sky_SunPosition` 0x461090 on the `.bin` fields the flare object 0x460780 passes."""
    m = rec.m
    raw = find(game_dir / 'GameData/Track/Sky', f'{track}.bin').read_bytes()
    g = lambda o: struct.unpack_from('<I', raw, o)[0]    # noqa: E731
    out = rec.alloc(16)
    m.call(SUN_POSITION, g(0xcdc), g(0xce0), g(0x3c), g(0x48), g(0x44), out, g(0xce4), budget=1_000_000)
    return m.get(out, 4)


def tracks(game_dir, poly_dir):
    rows = []
    for track, places in PLACEMENTS.items():
        rec = LightRecorder(game_dir)
        polys = rs.load_polygons(poly_dir / f'{track}-polygons.tsv')
        rec.build_support(polys)
        crp = ah.track_crp(game_dir / 'GameData/Track' / f'{track}.crp')
        rec.build_nodes(crp)
        rec.load_lighting(track, (game_dir / 'GameData/Track' / f'{track}.map').read_bytes())
        sun = track_sun(rec, game_dir, track)
        rec.m.put(rec.sky + 0x11c, *sun)
        header = rec.m.get(rec.tl, 20)
        rows.append(('track', track, '-', f'header={",".join(bits(v) for v in header)};'
                     f'sun={",".join(bits(v) for v in sun)}'))
        for j, (node, dx, dz, heading) in enumerate(places):
            rec.place(node, dx, dz, heading)
            m = rec.m
            loc_node = struct.unpack('<h', m.get_bytes(rs.CAR + 8, 2))[0]
            pos = m.car_get(0x330, 3)
            poly = rec.polygon_of(pos)
            S = rec.road_colour(pos)
            ls = rec.light_full()
            cid = f'{track}/{j}'
            rows.append(('place', cid, f'{node},{dx},{dz},{heading}',
                         f'pos={",".join(bits(v) for v in pos)};node={loc_node};roof={rec.roof:04x};'
                         f'covered={int(rec.roof & 0xc000 != 0)};poly={poly};'
                         f'poly_vertices={"|".join(",".join(bits(c) for c in q) for q in polys[poly][0]) if poly >= 0 else "-"};'
                         f'poly_colours={",".join(f"{c:04x}" for c in polys[poly][2]) if poly >= 0 else "-"};'
                         f'S={S};' + rec.state_text(ls)))
            print(f'{cid}: node {loc_node} poly {poly} roof {rec.roof:04x} S {S}', flush=True)
    return rows, 'GameData/Track .map/.crp/Sky .bin and support polygons (game data: keep in local/fixtures)'


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--game-dir', type=Path, default=vh.ROOT / 'local/game')
    ap.add_argument('--polygons', type=Path, default=vh.ROOT / 'local/derived/support')
    ap.add_argument('--out', type=Path, required=True)
    group = ap.add_mutually_exclusive_group(required=True)
    group.add_argument('--synthetic', action='store_true')
    group.add_argument('--tracks', action='store_true')
    args = ap.parse_args()
    rows, source = synthetic(args.game_dir) if args.synthetic else tracks(args.game_dir, args.polygons)
    lines = ['# replay-car-light.py: nfs5.exe 0x4746a0 / 0x43a8a6..0x43ad59 (0x43a560) / 0x446530; '
             + source + '; f32 as bits', 'kind\tcase\tkey\tout']
    lines += ['\t'.join(str(c) for c in r) for r in rows]
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{args.out}: {len(rows)} rows')


if __name__ == '__main__':
    main()

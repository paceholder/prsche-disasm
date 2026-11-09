#!/usr/bin/env python3
"""Record the original car planar sky-map layer (`R3DSkyMapTextureLayer`, tree / tunnel
shade on cars; change original-lighting, docs/re/lighting.md "Sky map on the car" in the
workspace root).

Executes nfs5.exe in Unicorn (vehicle_harness.py / replay-road-support.py, x87 control word
0x27f) and, for the texture binding, nfs5.exe with Drivers/dx7/dx7z.dll
(render_harness.py):

- `section` / `select` rows: `TrackLighting_Load` 0x4409d0 (thiscall, track name) runs for
  real on a `.map` text: the INI reader (0x5239b0 is answered by handing the text to the
  real parser 0x523980 -> 0x523c40; the file system is not run), the key readers 0x523e20 /
  0x523ad0 (CRT `atoi` 0x5a1f4a) / 0x523b20 (CRT `atof` 0x5a21b8) / 0x523bc0 (CRT `sscanf`),
  the section vector 0x441260 / 0x441440. Recorded: the 0x48-byte section records of the
  vector `+0x54` (start, end, texture{1,2,3}.open, texture{1,2,3}.covered FourCCs,
  texturesize, aspect). Then for every node `TrackLighting_SelectSection` 0x441140 (the
  selected section `+0x58`), the texture lookups 0x441180(covered 0/1, slot 0..2) and
  `texturesize` / `aspect` 0x4411c0 / 0x4411e0 (x87 ST0 after the call).
- `update` rows: `R3DCar_UpdateLighting` 0x43a560 (thiscall, R3DCar) from its entry to
  0x43a8a6, i.e. the whole sky-map part (the locator copy of car+8 and the overhead query
  0x473330 on it, run unmodified on the real support tree; 0x441140; the slots 0x441180 ->
  0x4408c0 (texture state 0x537a70 / 0x536470); 0x4411c0 / 0x4411e0; the offsets
  0x532330 / 0x532910 on the node graph [0x628bc0]; the node frame car+0x3b8 through
  0x539cd0 and 0x53a650 into light state +0x104). The light state is the real ctor 0x440780
  on R3DCar+0x2dc. The car is the 993 C4 of the road-support recorder placed on a node (plus a
  lateral / forward offset) and located by `Car_TrackLocate` 0x49af70 (owner polygon cleared
  first, so the locate copies the node frame as a polygon change does).
- `draw` rows: `R3DSkyMapTextureLayer` 0x445b10 (vtable 0x5b398c slot 0, thiscall: layer
  parameter block, output block) on a draw context [0x69e0d4] (vertex count +4, polygon size
  +0x20, flags +0x117e4, per-polygon skip words +0x11000, UV outputs +0x44c / +0x454, model
  matrix [[ctx]+0x5c]+4) and the light state [0x6115cc]: the three dependency channels
  (normal, position, optional byte index list), the slot parameter `**(param+4)`, the
  disable byte `+0x190`. Recorded: the texture written to the output block, the return
  value, the flags word and every UV pair written.
- `bind` rows (render machine): the texture state block of a texture made by the real
  0x5379d0, after 0x4408c0 (slot k), then the bind 0x536240 (thiscall on the view's texture
  pair) with the state setters of the static table filled by the real 0x5359a0
  (0x540480 / 0x535a60 / 0x535a90) -> THRASH_setstate -> dx7z.dll -> the logging D3D7
  device (SetTextureStageState calls).

Harness services, not game code: the heap behind the game's allocators (as
replay-road-support.py, plus the INI pool 0x531ca0); the env pack load 0x44d270 and the
texture registry ctor 0x44d0b0 do nothing; the registry lookup 0x44d320 (thiscall: 0,
FourCC) returns a texture object per FourCC (0x54 bytes, state block +0x38 from the real
0x5379d0, +4 = the FourCC as a marker): every FourCC a shipped `.map` names is in the
track's `.env` pack (exact four bytes, checked by `--tracks`); the road colour query 0x456160
(ambient part of 0x43a560, before the sky map) returns a zero block. CRT start-up:
`_cfltcvt_init` 0x5a0f5d (scanf's float conversion pointers).

    python scripts/research/replay-car-env-map.py --synthetic \\
        --out iterations/012-campaign-fidelity/crates/nfs-assets/tests/fixtures/car-env-map.tsv
    python scripts/research/replay-car-env-map.py --game-dir local/game --tracks \\
        --polygons local/derived/support --out local/fixtures/car-env-map-tracks.tsv
"""
import argparse
import importlib.util
import math
from pathlib import Path
import random
import struct
import sys

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import (UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP,
                               UC_X86_REG_FP0, UC_X86_REG_FPCW, UC_X86_REG_FPSW,
                               UC_X86_REG_FPTAG)

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import vehicle_harness as vh  # noqa: E402
import ai_harness as ah  # noqa: E402
import render_harness as rh  # noqa: E402

_spec = importlib.util.spec_from_file_location('rsupport', HERE / 'replay-road-support.py')
rs = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rs)
f32 = rs.vs.f32

TL_LOAD, SELECT, TEXTURE, TSIZE, ASPECT = 0x4409d0, 0x441140, 0x441180, 0x4411c0, 0x4411e0
INI_OPEN, INI_PARSE, INI_POOL = 0x5239b0, 0x523980, 0x531ca0
PACK_LOAD, REGISTRY_CTOR, REGISTRY_LOOKUP, ROAD_COLOUR = 0x44d270, 0x44d0b0, 0x44d320, 0x456160
CFLTCVT_INIT = 0x5a0f5d
TEX_STATE_INIT, SET_SLOT, TEX_BIND, TEX_STATE_TABLE_INIT = 0x5379d0, 0x4408c0, 0x536240, 0x5359a0
UPDATE_LIGHTING, UPDATE_STOP, LIGHT_CTOR, ROOF_RESULT = 0x43a560, 0x43a8a6, 0x440780, 0x43a6d9
SKY_LAYER = 0x445b10
METHOD_BUILD, LAYER_FACTORY_INIT = 0x44bf70, 0x449b40
LAYER_VTABLES = {0x5b398c: 'skymap', 0x5b39b4: 'sphere', 0x5b3964: 'shadowmap', 0x5b393c: 'light',
                 0x5bbc60: 'draw', 0x5b69bc: 'state', 0x5b3900: 'index'}
LIGHT_PTR, TRACK_LIGHTING_PTR, DRAW_CTX_PTR = 0x6115cc, 0x6115d8, 0x69e0d4
SHOWROOM, CARCHROME_FLAGS, RENDER_OBJ_PTR, VIEW_PTR, PATH_PREFIX = (
    0x65b928, 0x6197ca, 0x628130, 0x69ed0c, 0x65b2b8)
R3D_CAR_SIZE, LIGHT_OFFSET = 0x800, 0x2dc
CTX_SIZE = 0x12000
ENV0 = 0x30564e45
AFTER_DRAW = 0x7e1a0010       # marker: the layer after the pass's draw layer


def bits(x):
    return format(struct.unpack('<I', struct.pack('<f', x))[0], '08x')


def u32f(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


def fp0(uc):
    """x87 ST0 as a Python float. Unicorn's FP0..FP7 are the physical registers (ST0 is
    FP[TOP]) and return (mantissa, exponent)."""
    top = (uc.reg_read(UC_X86_REG_FPSW) >> 11) & 7
    mant, exp = uc.reg_read(UC_X86_REG_FP0 + top)
    sign = -1.0 if exp & 0x8000 else 1.0
    e = exp & 0x7fff
    if e == 0 and mant == 0:
        return 0.0 * sign
    return sign * mant * 2.0 ** (e - 16383 - 63)


def fourcc(v):
    return format(v & 0xffffffff, '08x')


class EnvRecorder(rs.SupportRecorder):
    """The road-support recorder with the track lighting object, an R3DCar and a draw
    context."""

    def __init__(self, game_dir):
        super().__init__(game_dir)
        m = self.m
        self.files = {}
        self.factory_ready = False
        self.textures = {}
        self.in_run = False
        m.stubs.pop(rs.vg.TRACK_QUERY, None)          # 0x473330 runs unmodified
        m.stubs[INI_POOL] = lambda m_: self.alloc(self.arg(1))
        for va in (PACK_LOAD, REGISTRY_CTOR):
            m.stubs[va] = rs.vg.thiscall(2, lambda *_: 0)

        def lookup(m_):
            sp = m_.uc.reg_read(UC_X86_REG_ESP)
            code = m_.get_u32(sp + 8)
            m_.put_u32(sp + 8, m_.get_u32(sp))
            m_.uc.reg_write(UC_X86_REG_ESP, sp + 8)
            return self.texture(code)
        m.stubs[REGISTRY_LOOKUP] = lookup
        self.road = self.alloc(0x40)
        m.stubs[ROAD_COLOUR] = rs.vg.thiscall(2, lambda *_: self.road)

        def ini_open(uc, _a, _s, _u):
            # thiscall (name), ret 4 -> the real parser 0x523980 (thiscall: text, size, ret 8)
            # on the file's bytes, with the text pointer stored at +4 as 0x5239b0 does.
            sp = uc.reg_read(UC_X86_REG_ESP)
            ret, name_ptr = struct.unpack('<II', bytes(uc.mem_read(sp, 8)))
            name = bytes(uc.mem_read(name_ptr, 256)).split(b'\0')[0].decode().lower()
            data = self.files[name]
            buf = self.alloc(len(data) + 1)
            uc.mem_write(buf, data)
            uc.mem_write(uc.reg_read(UC_X86_REG_ECX) + 4, struct.pack('<I', buf))
            uc.mem_write(sp - 4, struct.pack('<III', ret, buf, len(data)))
            uc.reg_write(UC_X86_REG_ESP, sp - 4)
            uc.reg_write(UC_X86_REG_EIP, INI_PARSE)
        m.uc.hook_add(UC_HOOK_CODE, ini_open, begin=INI_OPEN, end=INI_OPEN)
        self.roof = None

        def roof(uc, _a, _s, _u):
            self.roof = uc.reg_read(UC_X86_REG_EAX) & 0xffff
        m.uc.hook_add(UC_HOOK_CODE, roof, begin=ROOF_RESULT, end=ROOF_RESULT)
        m.call(CFLTCVT_INIT)
        self.prefix = self.alloc(16)
        self.render_obj = self.alloc(0x100)
        self.view_p, self.view_q = self.alloc(0x20), self.alloc(0x20)
        self.r3d = self.alloc(R3D_CAR_SIZE)
        m.call_thiscall(LIGHT_CTOR, self.r3d + LIGHT_OFFSET)
        self.ctx = self.alloc(CTX_SIZE)
        self.ctx_obj = self.alloc(0x80)
        self.model = self.alloc(0x40)
        self.globals()

    def globals(self):
        m = self.m
        m.put_u32(PATH_PREFIX, self.prefix)
        m.put_u32(SHOWROOM, 0)
        m.put_u32(RENDER_OBJ_PTR, self.render_obj)
        m.put_u32(VIEW_PTR, self.view_p)
        m.put_u32(self.view_p + 4, self.view_q)       # current textures +8 / +0xc: none
        m.put_bytes(CARCHROME_FLAGS, b'\x01\x01')      # CARCHROME 2 (0x6197ca / 0x6197cb)

    def texture(self, code):
        if code not in self.textures:
            if self.in_run:
                raise RuntimeError(f'texture {code:08x} not prepared')
            obj = self.alloc(0x54)
            self.m.call_thiscall(TEX_STATE_INIT, obj + 0x38)
            self.m.put_u32(obj + 4, code)
            self.textures[code] = obj
        return self.textures[code]

    def tex_name(self, ptr):
        return fourcc(self.m.get_u32(ptr + 4)) if ptr else '0'

    # -- track lighting ------------------------------------------------------
    def load_lighting(self, name, text):
        m = self.m
        self.files[f'{name}.map'.lower()] = text
        np = self.alloc(64)
        m.put_bytes(np, name.encode() + b'\0')
        obj = self.alloc(0x60)
        m.call_thiscall(TL_LOAD, obj, np, budget=100_000_000)
        for rec in self.section_records(obj):
            for code in rec[2:8]:
                self.texture(code)
        self.tl = obj
        return obj

    def section_records(self, obj):
        m = self.m
        vec = m.get_u32(obj + 0x54)
        b, e = m.get_u32(vec), m.get_u32(vec + 4)
        out = []
        for a in range(b, e, 0x48):
            raw = m.get_bytes(a, 0x48)
            out.append(list(struct.unpack('<8I', raw[:32])) + [raw[0x40:0x44].hex(), raw[0x44:0x48].hex(), a])
        return out

    def section_rows(self, case, obj):
        rows = []
        for k, r in enumerate(self.section_records(obj)):
            ts = struct.unpack('<I', bytes.fromhex(r[8]))[0]
            asp = struct.unpack('<I', bytes.fromhex(r[9]))[0]
            rows.append(('section', case, k, f'start={r[0]};end={r[1]};open={fourcc(r[2])},{fourcc(r[3])},'
                         f'{fourcc(r[4])};covered={fourcc(r[5])},{fourcc(r[6])},{fourcc(r[7])};'
                         f'size={ts:08x};aspect={asp:08x}'))
        head = self.m.get_bytes(obj, 0x50)
        rows.append(('header', case, '-', 'words=' + ','.join(f'{w:08x}' for w in struct.unpack('<20I', head))
                     + f';sel={self.m.get_u32(obj + 0x58):x}'))
        return rows

    def select(self, obj, node):
        """0x441140(node), 0x441180(c, k), 0x4411c0, 0x4411e0 -> (section index, names, ts, aspect)."""
        m = self.m
        recs = {r[10]: k for k, r in enumerate(self.section_records(obj))}
        m.call_thiscall(SELECT, obj, node & 0xffffffff)
        sel = m.get_u32(obj + 0x58)
        names = []
        for c in (0, 1):
            for k in range(3):
                names.append(self.tex_name(m.call_thiscall(TEXTURE, obj, c, k)))
        out = []
        for fn in (TSIZE, ASPECT):
            # The float10 return stays on the x87 stack: read ST0, then restore the stack.
            sw, tag = m.uc.reg_read(UC_X86_REG_FPSW), m.uc.reg_read(UC_X86_REG_FPTAG)
            m.call_thiscall(fn, obj)
            out.append(fp0(m.uc))
            m.uc.reg_write(UC_X86_REG_FPSW, sw)
            m.uc.reg_write(UC_X86_REG_FPTAG, tag)
        ts, asp = out
        return (recs[sel] if sel else -1), names, ts, asp

    def select_rows(self, case, obj, nodes):
        rows = []
        run = None
        for n in nodes:
            sel, names, ts, asp = self.select(obj, n)
            val = f'sel={sel};tex={",".join(names)};size={bits(ts)};aspect={bits(asp)}'
            if run and run[2] == val and run[1] == n - 1:
                run[1] = n
            else:
                if run:
                    rows.append(('select', case, f'{run[0]}..{run[1]}', run[2]))
                run = [n, n, val]
        if run:
            rows.append(('select', case, f'{run[0]}..{run[1]}', run[2]))
        return rows

    # -- light update ----------------------------------------------------------
    def place(self, node, dx, dz, heading):
        m = self.m
        self.setup_drive('993C4', node, 1, 0.0)
        nd = self.node(node)
        p = m.car_get(0x330, 3)
        p = [f32(p[i] + dx * nd['lat'][i] + dz * nd['fwd'][i]) for i in range(3)]
        m.car_put(0x330, *p)
        # Body turned by `heading` degrees about the node up (only the drawn model uses it).
        c, s = math.cos(math.radians(heading)), math.sin(math.radians(heading))
        f = [f32(c * nd['fwd'][i] + s * nd['lat'][i]) for i in range(3)]
        r = [f32(c * nd['lat'][i] - s * nd['fwd'][i]) for i in range(3)]
        m.car_put(0x364, *r, *nd['up'], *f)
        m.put_u32(rs.CAR + 0x30, 0)        # owner polygon none: the locate copies the node frame
        m.call(rs.TRACK_LOCATE, rs.CAR)

    def update(self):
        m = self.m
        m.put_u32(self.r3d + 0xf4, rs.CAR)
        m.put_u32(TRACK_LIGHTING_PTR, self.tl)
        self.globals()
        sp = vh.STACK_TOP - 0x1000
        m.put_u32(sp, vh.EXIT)
        m.uc.reg_write(UC_X86_REG_ESP, sp)
        m.uc.reg_write(UC_X86_REG_ECX, self.r3d)
        m.uc.reg_write(UC_X86_REG_FPCW, m.fpcw)
        self.roof = None
        self.in_run = True
        try:
            m.uc.emu_start(UPDATE_LIGHTING, UPDATE_STOP, count=20_000_000)
        finally:
            self.in_run = False
        if m.uc.reg_read(UC_X86_REG_EIP) != UPDATE_STOP:
            raise RuntimeError('0x43a560 did not reach 0x43a8a6')
        return self.r3d + LIGHT_OFFSET

    def light_text(self, ls):
        m = self.m
        g = m.get_u32
        return (f'slots={",".join(self.tex_name(g(ls + 0xd0 + 4 * k)) for k in range(3))};'
                f'k={g(ls):08x};inv_size={g(ls + 4):08x};aspect={g(ls + 8):08x};'
                f'pos={",".join(f"{g(ls + 0xe8 + 4 * i):08x}" for i in range(3))};'
                f'frame={",".join(f"{g(ls + 0x104 + 4 * i):08x}" for i in range(16))};'
                f'o={g(ls + 0x144):08x},{g(ls + 0x148):08x};off={m.get_bytes(ls + 0x190, 1)[0]}')

    # -- render methods ------------------------------------------------------------
    def method_layers(self, crp_base, entry_off, entry, caps, mt_flags):
        """0x44bf70 (thiscall on the `RMth` chunk entry: capability word, mt+0xc) -> the
        method's layer chain: (vtable name, attribute id, sub id, dependency indices)."""
        m = self.m
        if not self.factory_ready:
            m.call(LAYER_FACTORY_INIT)       # start-up: the game's layer factory 0x449b70
            self.factory_ready = True
        tag, lf, count, rel = entry
        obj = self.alloc(0x40)
        # The entry with its data pointer made absolute (bit 1 of +4: relative to the entry).
        data = crp_base + (entry_off if lf & 2 else 0) + rel
        m.put_bytes(obj, struct.pack('<IIII', tag, lf & ~2, count, data))
        m.call_thiscall(METHOD_BUILD, obj, caps, mt_flags, budget=100_000_000)
        meth = m.get_u32(obj + 0x14)
        g = m.get_u32
        order, layer = [], g(meth + 4)
        while layer and len(order) < 200:
            order.append(layer)
            layer = g(layer + 8)
        index = {a: k for k, a in enumerate(order)}
        out = []
        for a in order:
            n, arr = g(a + 0x2c), g(a + 0x28)
            deps = [index.get(g(arr + 4 * j), -1) if arr else -1 for j in range(n)]
            out.append((LAYER_VTABLES.get(g(a), f'{g(a):x}'), g(a + 0xc), g(a + 0x10), deps))
        return out

    def texture_state(self, obj):
        return ','.join(str(v) for v in struct.unpack('<7i', self.m.get_bytes(obj + 0x38, 28)))

    # -- layer draw --------------------------------------------------------------
    def draw(self, ls, case):
        """0x445b10 on the draw context; returns (ret, texture, flags, uv list)."""
        m = self.m
        verts = case['verts']                       # [(pos, normal)]
        n = len(verts)
        ctx = self.ctx
        m.put_bytes(ctx, bytes(CTX_SIZE))
        m.put_u32(ctx, self.ctx_obj)
        m.put_u32(self.ctx_obj + 0x60, self.model)
        m.put(self.model, *case['model'])
        index = case.get('index')
        m.put_u32(ctx + 4, len(index) if index is not None else n)
        m.put_u32(ctx + 0x20, case['poly'])
        m.put_u32(ctx + 0x117e4, case.get('flags', 0))
        for k in case.get('skip', ()):
            m.put_u32(ctx + 0x11000 + 8 * k, 1)
        stride = case.get('stride', 0x20)
        # The pass's layer chain after this layer (+8): a channel layer (id 5) and the pass's
        # draw layer (id 0, as 0x53f620) whose own +8 is a marker for "after the draw".
        layer, nxt, drw = self.alloc(0x30), self.alloc(0x30), self.alloc(0x30)
        m.put_u32(layer + 8, nxt)
        m.put_u32(nxt + 0xc, 5)
        m.put_u32(nxt + 8, drw)
        m.put_u32(drw + 0xc, 0)
        m.put_u32(drw + 8, AFTER_DRAW)
        m.put_u32(layer + 0xc, 3)
        m.put_u32(layer + 0x10, case.get('type', 0xe))
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
        normals = b''.join(struct.pack('<4f', *v[1], 0.0) for v in verts)
        points = b''.join(struct.pack('<4f', *v[0], 1.0) for v in verts)
        m.put_u32(deps, channel(normals, len(normals)))
        m.put_u32(deps + 4, channel(points, len(points)))
        m.put_u32(deps + 8, channel(bytes(index), len(index)) if index is not None else 0)
        param, value, out = self.alloc(8), self.alloc(4), self.alloc(8)
        m.put(value, case['param'])
        m.put_u32(param + 4, value)
        m.put_u32(out + 4, 0xdeadbeef)
        m.put_bytes(ls + 0x190, bytes([case.get('off', 0)]))
        m.put_u32(LIGHT_PTR, ls)
        m.put_u32(DRAW_CTX_PTR, ctx)
        # 0x612de0 transform cache serial: as left by previous draws (records are per vertex).
        out_base = ctx + (0x454 if case.get('type', 0xe) == 0xf else 0x44c)
        sentinel = 0x7fc0dead
        count = len(index) if index is not None else n
        for i in range(count):
            m.put_u32(out_base + i * stride, sentinel)
            m.put_u32(out_base + i * stride + 4, sentinel)
        self.in_run = True
        try:
            ret = m.call_thiscall(SKY_LAYER, layer, param, out, budget=5_000_000)
        finally:
            self.in_run = False
        uvs = []
        for i in range(count):
            a, b = m.get_u32(out_base + i * stride), m.get_u32(out_base + i * stride + 4)
            uvs.append('-' if (a, b) == (sentinel, sentinel) else f'{a:08x},{b:08x}')
        ret_name = {nxt: 'next', AFTER_DRAW: 'after_draw'}.get(ret, f'{ret:08x}')
        return ret_name, self.tex_name(m.get_u32(out + 4)), m.get_u32(ctx + 0x117e4), uvs


def draw_text(case):
    t = (f'param={bits(case["param"])};off={case.get("off", 0)};type={case.get("type", 0xe):x};'
         f'poly={case["poly"]};flags={case.get("flags", 0):x};stride={case.get("stride", 0x20):x};'
         f'skip={",".join(map(str, case.get("skip", ()))) or "-"};'
         f'index={",".join(map(str, case["index"])) if case.get("index") is not None else "-"};'
         f'model={",".join(bits(v) for v in case["model"])};'
         f'verts={"|".join(",".join(bits(c) for c in p + n) for p, n in case["verts"])}')
    return t


def draw_rows(rec, ls, cid, cases):
    rows = []
    for j, case in enumerate(cases):
        ret, tex, flags, uvs = rec.draw(ls, case)
        rows.append(('draw', cid, j, draw_text(case) + f';ret={ret};texture={tex};flags_out={flags:x};'
                     f'uv={"|".join(uvs)}'))
    return rows


def body_points(rng, n):
    """Car-sized body points (model space) with unit normals."""
    out = []
    for _ in range(n):
        p = (f32(rng.uniform(-0.9, 0.9)), f32(rng.uniform(-0.1, 1.3)), f32(rng.uniform(-2.2, 2.2)))
        v = [rng.gauss(0, 1) for _ in range(3)]
        ln = math.sqrt(sum(c * c for c in v)) or 1.0
        out.append((p, tuple(f32(c / ln) for c in v)))
    return out


def model_matrix(rot, pos):
    """Row-major model-to-world (rows: right, up, forward, translation) as the R3DCar draws."""
    r, u, f = rot
    return [*r, 0.0, *u, 0.0, *f, 0.0, *pos, 1.0]


def draw_cases(rng, rot, pos, params=(0.0, 0.5, 1.0)):
    verts = body_points(rng, 24)
    model = [f32(v) for v in model_matrix(rot, pos)]
    cases = []
    for p in params:
        cases.append(dict(verts=verts, model=model, param=p, poly=3))
    return cases


# -- synthetic inputs -------------------------------------------------------------
SYNTHETIC_MAPS = {
    # Defaults: no texture keys (ENV0), no texturesize / aspect (6, 1).
    'defaults': b'[header]\r\ncount=2\r\n\r\n[section0]\r\nstartslice=5\r\nendslice=9\r\n\r\n'
                b'[section1]\r\nstartslice=12\r\nendslice=12\r\ntexture2.open=827601497\r\n'
                b'texturesize=2.5\r\naspect=0.25\r\n',
    # Overlap (first match), a gap, a missing section index, count beyond the sections.
    'overlap': b'[header]\r\ncount=5\r\n[section0]\r\nstartslice=0\r\nendslice=0\r\n'
               b'texture1.open=538976305\r\ntexture1.covered=540103764\r\n'
               b'[section1]\r\nstartslice=10\r\nendslice=20\r\ntexture1.open=540103764\r\n'
               b'texture2.open=540103764\r\ntexture3.open=540169300\r\n'
               b'texture3.covered=827085908\r\ntexturesize=20.000000\r\naspect=1.000000\r\n'
               b'[section3]\r\nstartslice=15\r\nendslice=30\r\ntexture1.open=844378713\r\n'
               b'texturesize=6.406338\r\naspect=0.453674\r\n'
               b'[section2]\r\nstartslice=40\r\nendslice=35\r\n',
    # Duplicate keys, spaces, negative and huge numbers, garbage floats.
    'oddities': b'[header]\r\ncount=3\r\n[section0]\r\nstartslice=3\r\nendslice=8\r\n'
                b'texture1.open=538976305\r\ntexture1.open=540103764\r\ntexture2.open = 540103764\r\n'
                b'texture3.open=-1\r\ntexturesize=-107374176.000000\r\naspect=1e3\r\n'
                b'[section1]\r\nstartslice=-4\r\nendslice=4294967295\r\ntexture1.open=4294967295\r\n'
                b'[section2]\r\nstartslice=7\r\nendslice=7\r\ntexturesize=abc\r\naspect=\r\n',
    # count = 0: no sections, nothing selected.
    'empty': b'[header]\r\ncount=0\r\n[section0]\r\nstartslice=0\r\nendslice=100\r\n',
}


def synthetic_light(rec, rng, k):
    """A light state with the real ctor values and synthetic sky-map fields."""
    m = rec.m
    ls = rec.r3d + LIGHT_OFFSET
    names = [0x20202031, 0x20315454, 0x20325454]
    for j in range(3):
        m.put_u32(ls + 0xd0 + 4 * j, rec.texture(names[(j + k) % 3]))
    m.put(ls + 4, f32(1.0 / rng.choice([6.0, 20.0, 6.406338])))
    m.put(ls + 8, f32(rng.choice([1.0, 0.453674, 2.0])))
    m.put(ls + 0xe8, *[f32(rng.uniform(-500, 500)) for _ in range(3)])
    ang = rng.uniform(0, 2 * math.pi)
    c, s = math.cos(ang), math.sin(ang)
    lat, up, fwd = (c, 0.0, -s), (0.0, 1.0, 0.0), (s, 0.0, c)
    frame = [lat[0], up[0], fwd[0], 0, lat[1], up[1], fwd[1], 0, lat[2], up[2], fwd[2], 0, 0, 0, 0, 1]
    m.put(ls + 0x104, *[f32(v) for v in frame])
    m.put(ls + 0x144, f32(rng.uniform(-2, 2)), f32(rng.uniform(-2, 2)))
    return ls


def synthetic_draw_cases(rng, ls_pos):
    ang = rng.uniform(0, 2 * math.pi)
    c, s = math.cos(ang), math.sin(ang)
    rot = ((c, 0.0, -s), (0.0, 1.0, 0.0), (s, 0.0, c))
    pos = [f32(ls_pos[i] + rng.uniform(-0.5, 0.5)) for i in range(3)]
    verts = body_points(rng, 12)
    model = [f32(v) for v in model_matrix(rot, pos)]
    base = dict(verts=verts, model=model, poly=3)
    cases = []
    for p in (0.0, 0.0499, 0.05, 0.5, 0.9499, 0.95, 1.0, -1.0, 2.0):
        cases.append(dict(base, param=f32(p)))
    cases.append(dict(base, param=0.5, off=1))
    cases.append(dict(base, param=0.5, type=0xf))
    cases.append(dict(base, param=0.5, skip=(1, 3)))
    cases.append(dict(base, param=0.5, poly=1))
    cases.append(dict(base, param=0.5, flags=0x100))
    cases.append(dict(base, param=0.5, stride=0x24))
    idx = [0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7, 8, 9, 10, 10, 9, 11, 0, 3, 7]
    cases.append(dict(base, param=0.5, index=idx))
    cases.append(dict(base, param=1.0, index=idx, skip=(2,)))
    cases.append(dict(base, param=1.0, index=idx, flags=0x100))
    return cases


def synthetic(game_dir):
    rec = EnvRecorder(game_dir)
    rows = []
    for name, text in SYNTHETIC_MAPS.items():
        obj = rec.load_lighting(name, text)
        rows.append(('map', name, '-', text.decode('latin-1').replace('\r', '\\r').replace('\n', '\\n')))
        rows += rec.section_rows(name, obj)
        rows += rec.select_rows(name, obj, list(range(-2, 50)) + [4294967295, 65535, 32767])
    rng = random.Random(5)
    for k in range(4):
        ls = synthetic_light(rec, rng, k)
        cid = f'light{k}'
        rows.append(('light', cid, '-', rec.light_text(ls)))
        cases = synthetic_draw_cases(rng, rec.m.get(ls + 0xe8, 3))
        if k == 3:
            # An empty slot: the layer draws nothing (type 0xe returns 0x53f460's next).
            rec.m.put_u32(ls + 0xd4, 0)
            rows.append(('light', cid, '-', rec.light_text(ls)))
        rows += draw_rows(rec, ls, cid, cases)
    rows += bind_rows(game_dir)
    return rows, 'synthetic inputs (no game data)'


# -- texture binding (render machine) ------------------------------------------------
def bind_rows(game_dir):
    m = rh.RenderMachine(game_dir)
    D = rh.DATA
    heap = [D + 0x1000]

    def alloc(n):
        p = heap[0]
        heap[0] += (n + 15) & ~15
        m.put_bytes(p, bytes(n))
        return p
    m.put_u32(rh.DRIVER_DEVICE, m.device)
    m.put_u32(0x6bd97c, m.exports['_THRASH_setstate@8'])
    m.put_u32(0x6bd984, m.exports['_THRASH_getstate@4'])
    m.call(TEX_STATE_TABLE_INIT)
    vp, vq = alloc(0x20), alloc(0x20)
    m.put_u32(VIEW_PTR, vp)
    m.put_u32(vp + 4, vq)
    for va in (0x69dd1c, 0x69dd1d):
        m.put_bytes(va, b'\x01')
    m.put_bytes(0x5de6b8, b'\x00')          # no settexture: texture manager not modelled
    ls = alloc(0x200)
    rows = []
    for k in range(3):
        tex = alloc(0x54)
        m.call(TEX_STATE_INIT, ecx=tex + 0x38)
        before = ','.join(str(v) for v in struct.unpack('<7i', m.get_bytes(tex + 0x38, 28)))
        m.call(SET_SLOT, tex, k, ecx=ls)
        after = ','.join(str(v) for v in struct.unpack('<7i', m.get_bytes(tex + 0x38, 28)))
        cache = ','.join(str(m.get_i32(0x5de6c0 + 8 * j)) for j in range(7))
        m.log.clear()
        m.call(TEX_BIND, tex, 0, ecx=vq)
        d3d = '|'.join(f'{n}:{",".join(map(str, a))}' for n, a in m.log if n != 'BeginScene')
        slot = m.get_u32(ls + 0xd0 + 4 * k) == tex
        rows.append(('bind', 'tex', k, f'state_before={before};state_after={after};slot_set={int(slot)};'
                     f'cache_before={cache};d3d={d3d}'))
        # Rebind with the state cache primed (the second car / frame): only changes are sent.
        m.log.clear()
        m.call(TEX_BIND, 0, 0, ecx=vq)
        m.call(TEX_BIND, tex, 0, ecx=vq)
        d3d = '|'.join(f'{n}:{",".join(map(str, a))}' for n, a in m.log if n != 'BeginScene')
        rows.append(('bind', 'rebind', k, f'd3d={d3d}'))
        # Every cached stage-0 state invalid: the bind sends the texture's whole state block.
        for j in range(7):
            m.put_i32(0x5de6c0 + 8 * j, 0x7fffffff)
        m.log.clear()
        m.call(TEX_BIND, 0, 0, ecx=vq)
        m.call(TEX_BIND, tex, 0, ecx=vq)
        d3d = '|'.join(f'{n}:{",".join(map(str, a))}' for n, a in m.log if n != 'BeginScene')
        rows.append(('bind', 'full', k, f'd3d={d3d}'))
    return rows


# -- tracks --------------------------------------------------------------------------
TRACKS = ('alps', 'autobahn', 'canyon', 'castle', 'coastal', 'farmland', 'foothills', 'forest',
          'industrial', 'monaco1', 'monaco2', 'monaco3', 'monaco4', 'monaco5', 'skidpad')
# (track, node, lateral offset, forward offset, body heading in degrees)
PLACEMENTS = {
    'forest': [(700, 0.0, 0.0, 0.0), (700, 2.5, 1.0, 15.0), (700, -3.0, -2.0, -30.0), (701, 1.0, 0.5, 90.0),
               (1300, 0.5, 0.0, 5.0), (1950, 1.0, 1.0, 0.0), (300, 0.0, 0.0, 0.0), (1630, 0.0, 0.0, 0.0),
               (5, 0.0, 0.0, 0.0), (600, 0.0, 0.0, 0.0)],
    'farmland': [(70, 0.0, 0.0, 0.0), (70, 2.0, -1.5, 20.0), (71, -2.5, 0.5, -45.0), (300, 1.0, 0.0, 0.0),
                 (900, 0.0, 0.0, 0.0)],
    'coastal': [(190, 0.0, 0.0, 0.0), (190, 2.5, 1.0, 10.0), (193, -1.5, 0.0, -10.0), (198, 0.0, 2.0, 0.0),
                (0, 0.0, 0.0, 0.0), (10, 1.0, 0.0, 0.0), (100, 0.0, 0.0, 0.0), (400, 0.0, 0.0, 0.0)],
}


def env_names(game_dir, track):
    path = game_dir / 'GameData/Track' / f'{track}.env'
    data = path.read_bytes()
    if data[1] == 0xfb and data[0] & 0xfe == 0x10:
        data = ah.refpack(data)
    n = struct.unpack_from('<I', data, 8)[0]
    return {struct.unpack_from('<I', data, 16 + 8 * i)[0] for i in range(n)}


def tracks(game_dir, poly_dir):
    rows = []
    rec = EnvRecorder(game_dir)
    for track in TRACKS:
        text = (game_dir / 'GameData/Track' / f'{track}.map').read_bytes()
        obj = rec.load_lighting(track, text)
        names = env_names(game_dir, track)
        missing = {c for r in rec.section_records(obj) for c in r[2:8]} - names
        if missing:
            raise RuntimeError(f'{track}: .map FourCCs missing from the .env pack: {missing}')
        rows += rec.section_rows(track, obj)
        crp = ah.track_crp(game_dir / 'GameData/Track' / f'{track}.crp')
        n, _nodes, _sections = ah.sim_nodes(crp)
        rows.append(('nodes', track, '-', f'count={n}'))
        rows += rec.select_rows(track, obj, list(range(-1, n + 2)))
        print(f'{track}: {len(rec.section_records(obj))} sections, {n} nodes', flush=True)
    rows += method_rows(rec, game_dir)
    rng = random.Random(7)
    for track, places in PLACEMENTS.items():
        rec = EnvRecorder(game_dir)
        rec.build_support(rs.load_polygons(poly_dir / f'{track}-polygons.tsv'))
        crp = ah.track_crp(game_dir / 'GameData/Track' / f'{track}.crp')
        rec.build_nodes(crp)
        obj = rec.load_lighting(track, (game_dir / 'GameData/Track' / f'{track}.map').read_bytes())
        for j, (node, dx, dz, heading) in enumerate(places):
            rec.place(node, dx, dz, heading)
            m = rec.m
            loc_node = struct.unpack('<h', m.get_bytes(rs.CAR + 8, 2))[0]
            nd = rec.node(loc_node)
            cache = rec.node_code(m.get_u32(rs.CAR + 0x34))     # locator cell cache (+0x2c)
            ls = rec.update()
            cid = f'{track}/{j}'
            car = (f'pos={",".join(bits(v) for v in m.car_get(0x330, 3))};node={loc_node};'
                   f'car_frame={",".join(bits(v) for v in m.car_get(0x3b8, 9))};'
                   f'node_pos={",".join(bits(v) for v in nd["pos"])};'
                   f'node_lat={",".join(bits(v) for v in nd["lat"])};'
                   f'node_fwd={",".join(bits(v) for v in nd["fwd"])};roof={rec.roof:04x};'
                   f'covered={int(rec.roof & 0xc000 != 0)};cache={cache}')
            rows.append(('update', cid, f'{node},{dx},{dz},{heading}', car + ';' + rec.light_text(ls)))
            rot = (m.car_get(0x364, 3), m.car_get(0x370, 3), m.car_get(0x37c, 3))
            rows += draw_rows(rec, ls, cid, draw_cases(rng, rot, m.car_get(0x330, 3)))
            print(f'{cid}: node {loc_node} roof {rec.roof:04x} {rec.light_text(ls)[:60]}', flush=True)
    return rows, 'GameData/Track .map/.env/.crp and support polygons (game data: keep in local/fixtures)'


def crp_entries(data):
    """(offset, (id, lf, count, rel)) of a car CRP's table (articles then the rest)."""
    ac, mc, start = struct.unpack_from('<I', data, 4)[0] >> 5, *struct.unpack_from('<II', data, 8)
    start *= 16
    return [(start + 16 * i, struct.unpack_from('<IIII', data, start + 16 * i)) for i in range(ac + mc)]


def method_rows(rec, game_dir):
    """Per car: every `mt` record (method name, +0xc flags) and the layer chain 0x44bf70 builds
    for each method / flags pair at capability words 1 (CARCHROME 0) and 3 (1 and 2)."""
    rows = []
    for path in sorted((game_dir / 'GameData/CarModel').glob('*.crp')):
        data = path.read_bytes()
        if data[:4] != b' raC':
            data = ah.refpack(data)
        car = path.stem.lower()
        entries = crp_entries(data)
        base = rec.alloc(len(data) + 16)
        rec.m.put_bytes(base, data)
        methods, mts = {}, []
        for off, (ident, lf, count, rel) in entries:
            if lf >> 8 == 0:
                continue
            p = (off if lf & 2 else 0) + rel
            if lf & 1 and ident >> 16 == 0x6d74:                      # 'mt', indexed
                name = data[p + 16:p + 32].split(b'\0')[0].decode('latin-1')
                mts.append((ident & 0xffff, name, struct.unpack_from('<I', data, p + 12)[0]))
            elif ident == 0x524d7468:                                 # 'RMth'
                methods[data[p:p + 16].split(b'\0')[0].decode('latin-1')] = (off, (ident, lf, count, rel))
        for index, name, flags in sorted(mts):
            rows.append(('mt', car, index, f'method={name};flags={flags:x}'))
        for name, flags in sorted({(n, f) for _, n, f in mts}):
            if name not in methods:
                rows.append(('method', car, f'{name}/{flags:x}', 'missing'))
                continue
            for caps in (1, 3):
                layers = rec.method_layers(base, *methods[name], caps, flags)
                text = '|'.join(f'{v}:{a:x}:{b:x}:{",".join(map(str, d))}' for v, a, b, d in layers)
                rows.append(('method', car, f'{name}/{flags:x}/{caps}', text))
        print(f'{car}: {len(mts)} materials, methods {sorted(methods)}', flush=True)
    return rows


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--game-dir', type=Path, default=vh.ROOT / 'local/game')
    ap.add_argument('--polygons', type=Path, default=vh.ROOT / 'local/derived/support')
    ap.add_argument('--out', type=Path, required=True)
    group = ap.add_mutually_exclusive_group(required=True)
    group.add_argument('--synthetic', action='store_true')
    group.add_argument('--tracks', action='store_true')
    args = ap.parse_args()
    if args.synthetic:
        rows, source = synthetic(args.game_dir)
    else:
        rows, source = tracks(args.game_dir, args.polygons)
    lines = ['# replay-car-env-map.py: nfs5.exe 0x4409d0 / 0x441140 / 0x441180 / 0x43a560..0x43a8a6 / '
             '0x445b10 / 0x4408c0 + 0x536240 -> dx7z.dll; ' + source + '; f32 as bits, FourCCs as LE words',
             'kind\tcase\tkey\tout']
    lines += ['\t'.join(str(c) for c in r) for r in rows]
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{args.out}: {len(rows)} rows')


if __name__ == '__main__':
    main()

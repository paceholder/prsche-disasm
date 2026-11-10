#!/usr/bin/env python3
"""Record the original car render-method draw from the scene loader down to Direct3D 7: pass 1
(the lit body: transform, R3DLightLayer, texture, draw), pass 4 (sphere map) and pass 6 (the
planar sky map: tree shade, tunnel lamps), change original-lighting (docs/re/lighting.md
"Car draw recorded" in the workspace root).

Executes nfs5.exe and Drivers/dx7/dx7z.dll in Unicorn (render_harness.RenderMachine plus the
PACODE vector library, the machine of replay-sky-draw.py) on real car data:

- Scene: the real car scene loader 0x44a770 (thiscall on the material table holder, the CRP)
  builds every `mt` record's render method from the car's `RMth` chunks (0x44bbe0 /
  0x44bb70 / 0x44bf70 with the layer factory 0x449b70 registered by 0x449b40, the method
  cache [0x61976c] from 0x44b960, capability word [0x6197f8] = 3: CARCHROME 1 or 2) and
  the material records (0x1c bytes: texture, index, entry count, entries, method, `mt`).
- Light state: `R3DCarLight` ctor 0x440780 on R3DCar+0x2dc ([0x6115cc]), then the words
  +0x18..+0xd0 and +0x190 recorded by replay-car-light.py (`place` rows of
  local/fixtures/car-light-tracks.tsv) and the sky-map fields +0..+0x148 / slots +0xd0..
  recorded by replay-car-env-map.py (`update` rows of local/fixtures/car-env-map-tracks.tsv)
  for the same placement; the sphere-map slots +0xdc.. (R3DCar+0x3b8, `Car_Load` 0x441a10:
  SPC1, SPC2, SPC3) and the sky-map slots (set by the real slot setter 0x4408c0, as 0x43a560
  does) are harness texture objects (below).
- Model matrix as `R3DCar_DrawBody` 0x4432b0 builds it: push 0x539ec0, scale 0x539b30
  (-1, 1, -1, 1), rotation and position 0x539cd0 (car+0x364 rows, car+0x330), 0x539ad0 twice;
  the camera (0x4672f0 / 0x467230 / 0x467120 / 0x466220 / 0x466f30, near 0.15, far 700,
  640 x 480) looks at the car from a list of eye offsets in the car frame.
- Draw: per `pr` group of level 1 of the drawn articles (as `R3DCarLOD_DrawParts` 0x43c830:
  vertex count = the `pr` entry's count, `pr` data = the group), the real channel binder
  0x44a250 (thiscall on the material record: count, part, group; part = the article entry and
  the end of its table, the per-type lookup caches 0x43c7d0) and the method's draw (vtable
  0x5b3c94 slot 1 = 0x53f1b0: params, polygon size, count) with every layer: channels
  0x44a0a0, `LayerTransformIndexCoords` 0x449390 (0x540bc0 -> PACODE 0x5385b0, clip flags
  0x100 / 0x200, back faces 0x200 skip words), `LayerIndexedUV` 0x449910, the render state
  0x53f780 -> 0x540660, textures 0x53f4a0 -> 0x536240 -> THRASH_settexture, 0x446530,
  0x445520, 0x445b10, 0x446130, the draws 0x53f620 -> 0x540ce0 -> 0x544a00 ->
  THRASH_drawtrimesh -> dx7z.dll batch -> the logging D3D7 device.
  Recorded per group: the layer chain as run (vtable, return), at every draw layer
  (0x53f620 entry: the pass) the draw-context flags +0x117e4, skip words and the 32-byte
  vertex slots +0x434 (x, y, z, rhw, diffuse, specular, u, v), the model-space sources of
  every slot (position and normal through the light layer's and the sky layer's
  dependency channels), and every D3D call (SetRenderState, SetTextureStageState,
  SetTexture, DrawIndexedPrimitive with its vertex buffer).

Harness services, not game code: the heap (0x59ef90, 0x59eff0, 0x59ecb0, 0x4211d0; frees
no-ops; pool refill 0x59eeb0 as replay-road-support.py), critical sections 0x5322b0 /
0x5322c0; the texture registry 0x44d320 (thiscall: name hi, FourCC) returns one texture
object per name (state block from the real 0x5379d0) and 0x44d380 (create) copies the
requested state block: textures are resident (record +8, surface marker 0x7e000000 | id) as
in replay-sky-draw.py, so SetTexture shows which texture is bound (ids in the `texture`
rows). The THRASH state caches (nfs5 0x5ded34 and the texture-state cache 0x5de6c0) are
invalidated before every group so each group shows its full state.

    python scripts/research/replay-car-draw.py --game-dir local/game \\
        --out local/fixtures/car-draw-cars.tsv
"""
import argparse
import importlib.util
import math
from pathlib import Path
import struct
import sys

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EAX

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import render_harness as rh  # noqa: E402
import ai_harness as ah  # noqa: E402

_spec = importlib.util.spec_from_file_location('sky_draw', HERE / 'replay-sky-draw.py')
sd = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(sd)
sl = sd.sl

CRP, CRP_SIZE = 0x4000000, 0x1000000
BIG, BIG_SIZE = 0x5000000, 0x3000000
SCENE_LOAD, METHOD_CACHE_CTOR, METHOD_CACHE, CAPS = 0x44a770, 0x44b960, 0x61976c, 0x6197f8
LIGHT_CTOR, LIGHT_PTR, BIND = 0x440780, 0x6115cc, 0x44a250
CHUNK_FIND, SET_SLOT = 0x59f7c0, 0x4408c0
PUSH, SCALE, SET_RT, MUL_RIGHT, MARK = 0x539ec0, 0x539b30, 0x539cd0, 0x539ad0, 0x539f40
POOL_PTR, POOL_REFILL = 0x5e4fe8, 0x59eeb0
DRAW_LAYER_VT, LOOP_CALL, LOOP_RET = 0x5bbc60, 0x53f221, 0x53f223
LAYER_NAMES = {0x5b3aa4: 'channel', 0x5b3900: 'transform', 0x5b3a2c: 'uv', 0x5b69bc: 'state',
               0x5b69e4: 'param', 0x5b6a0c: 'texture', 0x5bbdec: 'param6', 0x5b393c: 'light',
               0x5bbc60: 'draw', 0x5b39b4: 'sphere', 0x5b398c: 'skymap', 0x5b3964: 'shadowmap',
               0x5b3a7c: 'transform0'}
SURFACE_MARK = 0x7e000000
SPC = (0x31435053, 0x32435053, 0x33435053)      # 'SPC1'..'SPC3' (LE words)
NEAR, FAR = 0.15, 700.0


def bits(x):
    return format(struct.unpack('<I', struct.pack('<f', x))[0], '08x')


def u32f(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


def unbits(h):
    return struct.unpack('<f', struct.pack('<I', int(h, 16)))[0]


def entries(d, o, n):
    out = []
    for j in range(n):
        e = o + 16 * j
        ident, lf, cnt, rel = struct.unpack_from('<IIII', d, e)
        tag = (ident >> 16).to_bytes(2, 'big') if lf & 1 else ident.to_bytes(4, 'big')
        out.append(dict(off=e, tag=tag, idx=ident & 0xffff if lf & 1 else -1, count=cnt, size=lf >> 8,
                        data=((e if lf & 2 else 0) + rel) if lf >> 8 else e + rel * 16))
    return out


def articles(d):
    ac = struct.unpack_from('<I', d, 4)[0] >> 5
    mc, start = struct.unpack_from('<II', d, 8)
    top = entries(d, start * 16, ac + mc)
    arts = []
    for a in top[:ac]:
        kids = entries(d, a['data'], a['count'])
        name = next((d[k['data']:k['data'] + k['size']].split(b'\0')[0] for k in kids if k['tag'] == b'Name'), b'')
        arts.append(dict(entry=a, kids=kids, name=name.decode('latin-1').strip()))
    mts = {e['idx']: e for e in top[ac:] if e['tag'] == b'mt'}
    return arts, mts


class CarDraw(sd.DrawRecorder):
    def __init__(self, game_dir):
        super().__init__(game_dir)
        m = self.m
        m.uc.mem_map(CRP, CRP_SIZE)
        m.uc.mem_map(BIG, BIG_SIZE)
        self.reset()
        self.top = BIG

        def args(k):
            sp = m.uc.reg_read(UC_X86_REG_ESP)
            return list(struct.unpack(f'<{k}I', m.get_bytes(sp + 4, 4 * k)))
        self.args = args
        for va in (0x59ef90, 0x59eff0, 0x4211d0, 0x59ecb0):
            m.stubs[va] = ('alloc', 0, lambda m_, _a: self.alloc(args(1)[0]))
        for va in (0x59f050, 0x4237d0, 0x59ecd0, 0x5322b0, 0x5322c0):
            m.stubs[va] = ('nop', 0, lambda m_, _a: 0)
        self.pool = self.alloc(0x1000)
        m.put_u32(POOL_PTR, self.pool)

        def refill(m_, _a):
            bucket = args(1)[0]
            size, head = 8 * (bucket + 1), 0
            for _ in range(1024):
                b = self.alloc(size)
                m.put_u32(b + 4, head)
                head = b
            m.put_u32(self.pool + 0x34 + 4 * bucket, head)
            return 0
        m.stubs[POOL_REFILL] = ('refill', 0, refill)
        self.texture_ids = {}
        self.pending = []
        m.stubs[0x44d320] = ('reg.lookup', 2, lambda m_, a: self.texture((a[0], a[1])))

        def create(m_, a):
            obj = self.texture(('create', a[1], a[2], len(self.texture_ids)))
            m.put_bytes(obj + 0x38, m.get_bytes(a[3], 28))
            return obj
        m.stubs[0x44d380] = ('reg.create', 7, create)

    def alloc(self, n):
        p = self.top
        self.top = (self.top + max(1, n) + 15) & ~15
        if self.top > BIG + BIG_SIZE:
            raise RuntimeError('harness heap exhausted')
        self.m.put_bytes(p, bytes(max(1, n)))
        return p

    def texture(self, key):
        """A texture object per key: +4 = the FourCC (harness marker); the state block +0x38 is
        filled by the real 0x5379d0 after the run that asked for it (no nested emulation)."""
        if key not in self.texture_ids:
            obj = self.alloc(0x54)
            self.m.put_u32(obj + 4, key[1] if len(key) > 1 else 0)
            self.texture_ids[key] = (len(self.texture_ids), obj)
            self.pending.append(obj)
        return self.texture_ids[key][1]

    def finish_textures(self, init=True):
        m = self.m
        for obj in self.pending:
            if init and m.get_bytes(obj + 0x38, 28) == bytes(28):
                m.call(0x5379d0, ecx=obj + 0x38)
            tid = next(i for (i, o) in self.texture_ids.values() if o == obj)
            rec, drv = self.alloc(0x40), self.alloc(0x40)
            m.put_u32(rec + 0x10, drv)
            m.put_u32(rec + 0x2c, 2)
            m.put_u32(drv + 0x34, SURFACE_MARK | tid)
            m.put_u32(obj + 8, rec)
        self.pending = []

    # -- scene ---------------------------------------------------------------------
    def load_car(self, data):
        m = self.m
        m.put_bytes(CRP, data)
        cache = self.alloc(8)
        m.call(METHOD_CACHE_CTOR, ecx=cache)
        m.put_u32(METHOD_CACHE, cache)
        m.put_u32(CAPS, 3)
        self.holder = self.alloc(0x40)
        m.call(SCENE_LOAD, CRP, ecx=self.holder, budget=1_000_000_000)
        self.finish_textures()
        self.materials = m.get_u32(self.holder)
        self.cam = self.setup_draw()
        self.cam_ctx = m.get_u32(self.cam + 0xc)
        self.stack = self.cam_ctx + 0x5c
        # [[0x69ed0c]]+0x14: the camera context whose +0x5c stack 0x449390 / 0x445520 read.
        m.put_u32(sl.VIEW_Q + 0x14, self.cam_ctx)
        self.r3d = self.alloc(0x800)
        self.ls = self.r3d + 0x2dc
        m.call(LIGHT_CTOR, ecx=self.ls)
        m.put_u32(LIGHT_PTR, self.ls)
        for k, code in enumerate(SPC):
            m.put_u32(self.ls + 0xdc + 4 * k, self.texture(('SPC', code)))
        self.finish_textures()

    def set_light(self, light_words, env):
        m = self.m
        ls = self.ls
        m.put_bytes(ls + 0x18, struct.pack(f'<{len(light_words["state"])}I', *light_words['state']))
        m.put_bytes(ls + 0x190, bytes([light_words['mirror']]))
        g = lambda k: [int(w, 16) for w in env[k].split(',')]   # noqa: E731
        m.put_u32(ls, g('k')[0])
        m.put_u32(ls + 4, g('inv_size')[0])
        m.put_u32(ls + 8, g('aspect')[0])
        m.put_bytes(ls + 0xe8, struct.pack('<3I', *g('pos')))
        m.put_bytes(ls + 0x104, struct.pack('<16I', *g('frame')))
        m.put_bytes(ls + 0x144, struct.pack('<2I', *g('o')))
        m.put_bytes(ls + 0x190, bytes([int(env['off'])]))
        codes = [int(w, 16) for w in env['slots'].split(',')]
        objs = [self.texture(('env', c)) if c else 0 for c in codes]
        self.finish_textures()
        # The slot setter 0x4408c0 (thiscall on the light state: texture, slot) as 0x43a560
        # calls it: stores the slot and sets the texture's address words (wrap).
        for k, obj in enumerate(objs):
            m.call(SET_SLOT, obj, k, ecx=ls)

    def set_model(self, rot, pos):
        m = self.m
        g = m.get_u32
        # Back to the stack base, then 0x4432b0's sequence.
        m.put_u32(self.stack + 8, 0)
        m.put_u32(self.stack + 4, g(self.stack))
        m.call(PUSH, ecx=self.stack)
        cur = g(self.stack + 4)
        s, rt, rp, pp = self.alloc(64), self.alloc(64), self.alloc(36), self.alloc(12)
        m.call(SCALE, u32f(-1.0), u32f(1.0), u32f(-1.0), u32f(1.0), ecx=s)
        m.put_bytes(rp, struct.pack('<9I', *[u32f(v) for v in rot]))
        m.put_bytes(pp, struct.pack('<3I', *[u32f(v) for v in pos]))
        m.call(SET_RT, rp, pp, ecx=rt)
        m.call(MUL_RIGHT, rt, ecx=s)
        m.call(MARK, ecx=self.stack)
        m.call(MUL_RIGHT, s, ecx=cur)
        return struct.unpack('<16I', m.get_bytes(cur, 64))

    def set_camera(self, eye, target):
        m = self.m
        f = [target[i] - eye[i] for i in range(3)]
        n = math.sqrt(sum(c * c for c in f))
        f = [c / n for c in f]
        r = [f[2], 0.0, -f[0]]
        n = math.sqrt(sum(c * c for c in r))
        r = [c / n for c in r]
        u = [f[1] * r[2] - f[2] * r[1], f[2] * r[0] - f[0] * r[2], f[0] * r[1] - f[1] * r[0]]
        rot = [struct.unpack('<f', struct.pack('<f', v))[0] for v in (*r, *u, *f)]
        m.put_bytes(self.cam + 0xe8, struct.pack('<9f', *rot))
        m.call(sd.CAMERA_ACTIVATE, self.cam, ecx=sl.RENDER_OBJ)
        m.call(sd.CAMERA_NEAR_FAR, u32f(NEAR), u32f(FAR), ecx=self.cam)
        m.put_bytes(sd.POS, struct.pack('<3f', *eye))
        m.call(sd.CAMERA_POSITION, sd.POS, ecx=self.cam + 0xb8)
        m.call(sd.CAMERA_UPDATE, ecx=self.cam)
        return rot

    # -- draw ------------------------------------------------------------------------
    def channel(self, layer, j):
        """Dependency j of a layer -> (size, pointer) of its channel."""
        m = self.m
        if m.get_u32(layer + 0x2c) <= j or not m.get_u32(layer + 0x28):
            return None
        d = m.get_u32(m.get_u32(layer + 0x28) + 4 * j)
        if not d:
            return None
        arr = m.get_u32(m.get_u32(d + 0x20) + 8)
        a = m.get_u32(d + 0x1c)
        return m.get_u32(arr + 8 * a), m.get_u32(arr + 8 * a + 4)

    def draw_group(self, part, grp, count, mat):
        m = self.m
        g = m.get_u32
        rec = g(self.materials + 4 * mat)
        for k in range(6):
            m.put_i32(sl.CACHE_VALUES + 8 * k, 0x7fffffff)
        for k in range(7):
            m.put_i32(0x5de6c0 + 8 * k, 0x7fffffff)
        poly = m.call(BIND, count, part, grp, ecx=rec, budget=20_000_000)
        obj = g(rec + 0x14)
        meth, params = g(obj + 0x14), g(obj + 0x2c)
        trace, passes = [], []
        ctxp = g(0x69e0d4)
        state = {}

        def on_call(uc, _a, _s, _u):
            layer = uc.reg_read(UC_X86_REG_ECX)
            vt = g(layer)
            state['layer'] = layer
            if vt == DRAW_LAYER_VT:
                passes.append(self.snapshot(ctxp, layer, state, count))
            trace.append([LAYER_NAMES.get(vt, f'{vt:x}'), g(layer + 0xc), g(layer + 0x10), g(layer + 0x18)])
            if vt == 0x5b393c:
                state['light'] = layer
                inp = params + 8 * g(layer + 0x18)
                state['light_param'] = m.get_bytes(g(inp + 4), 12).hex()
            elif vt == 0x5b398c:
                state['sky'] = layer
                state['sky_indexed'] = int(self.channel(layer, 2) is not None)

        def on_ret(uc, _a, _s, _u):
            r = uc.reg_read(UC_X86_REG_EAX)
            trace[-1].append(r and LAYER_NAMES.get(g(r), f'{g(r):x}') or 'end')
            trace[-1].append(g(ctxp + 0x117e4))
        h1 = m.uc.hook_add(UC_HOOK_CODE, on_call, begin=LOOP_CALL, end=LOOP_CALL)
        h2 = m.uc.hook_add(UC_HOOK_CODE, on_ret, begin=LOOP_RET, end=LOOP_RET)
        self.events = []
        try:
            m.call(g(g(meth) + 4), params, poly, count, ecx=meth, budget=400_000_000)
            m.call(sl.DRIVER_FLUSH)
        finally:
            m.uc.hook_del(h1)
            m.uc.hook_del(h2)
        events, self.events = self.events, None
        sources = self.sources(state, count)
        sources['light_param'] = state.get('light_param', '-')
        sources['sky_indexed'] = str(state.get('sky_indexed', '-'))
        return poly, trace, passes, sources, events

    def snapshot(self, ctxp, layer, state, count):
        m = self.m
        flags = m.get_u32(ctxp + 0x117e4)
        stride = m.get_u32(layer + 0x14) or 0x20
        poly = max(1, m.get_u32(ctxp + 0x20))
        skips = ''.join('1' if m.get_u32(ctxp + 0x11000 + 8 * p) else '0' for p in range(count // poly))
        slots = m.get_bytes(ctxp + 0x434, stride * count)
        return dict(flags=flags, stride=stride, skips=skips, slots=slots, after=len(state))

    def sources(self, state, count):
        """Model-space normal per slot through the light layer (normals dep 0, index dep 2)
        and position + normal through the sky layer (normal 0, position 1, index 2)."""
        m = self.m
        out = {}
        for name, (ni, pi, ii) in (('light', (0, None, 2)), ('sky', (0, 1, 2))):
            layer = state.get(name)
            if not layer:
                continue
            nrm = self.channel(layer, ni)
            pos = self.channel(layer, pi) if pi is not None else None
            idx = self.channel(layer, ii)
            index = list(m.get_bytes(idx[1], count)) if idx and idx[1] else list(range(count))
            rows = []
            for i in index:
                n = struct.unpack('<3I', m.get_bytes(nrm[1] + 16 * i, 12))
                p = struct.unpack('<3I', m.get_bytes(pos[1] + 16 * i, 12)) if pos else ()
                rows.append(','.join(f'{w:08x}' for w in p + n))
            out[name] = '|'.join(rows)
        return out


def event_text(ev):
    return sd.event_text(ev)


def read_rows(path, kind):
    out = {}
    for line in path.read_text().splitlines():
        c = line.split('\t')
        if len(c) == 4 and c[0] == kind:
            out[(c[1].split('/')[0], c[2])] = dict(p.split('=', 1) for p in c[3].split(';') if '=' in p)
    return out


CARS = ('356a', '993')
PLACES = [('forest', '700,0.0,0.0,0.0'), ('coastal', '190,0.0,0.0,0.0'), ('forest', '5,0.0,0.0,0.0')]
# Eye offsets in the car frame (lateral, up, forward), looking at (0, 0.6, 0) of the car:
# tail camera and a close side view with the car partly off screen.
EYES = [(0.0, 1.9, -6.5), (1.6, 0.9, 0.6)]
PARTS = ('Body', 'Roof', 'HoodOut', 'DoorOut', 'TrunkOut', 'DoorWindow1', 'Windshield', 'Window',
         'Wheel1', 'Interior')


def record(game_dir, fixtures):
    light = read_rows(fixtures / 'car-light-tracks.tsv', 'place')
    env = read_rows(fixtures / 'car-env-map-tracks.tsv', 'update')
    rows = []
    for car in CARS:
        path = next(p for p in (game_dir / 'GameData/CarModel').iterdir() if p.name.lower() == f'{car}.crp')
        data = path.read_bytes()
        if data[:4] != b' raC':
            data = ah.refpack(data)
        arts, mts = articles(data)
        rec = CarDraw(game_dir)
        rec.load_car(data)
        g = rec.m.get_u32
        for key, (tid, obj) in rec.texture_ids.items():
            rows.append(('texture', car, tid, f'key={key[0]}:{",".join(f"{v:x}" for v in key[1:])};'
                         f'state={",".join(str(v) for v in struct.unpack("<7i", rec.m.get_bytes(obj + 0x38, 28)))}'))
        drawn = [a for a in arts if any(a['name'].lower().startswith(p.lower()) for p in PARTS)]
        for track, key in PLACES:
            lw, ev = light[(track, key)], env[(track, key)]
            words = dict(state=[int(w, 16) for w in lw['state'].split(',')], mirror=int(lw['mirror']))
            rec.set_light(words, ev)
            pos = [unbits(h) for h in ev['pos'].split(',')]
            fr = [unbits(h) for h in ev['car_frame'].split(',')]
            rot = fr                                   # heading 0: body axes = node frame
            model = rec.set_model(rot, pos)
            for e, eye_off in enumerate(EYES):
                lat, up, fwd = fr[0:3], fr[3:6], fr[6:9]
                eye = [pos[i] + eye_off[0] * lat[i] + eye_off[1] * up[i] + eye_off[2] * fwd[i] for i in range(3)]
                target = [pos[i] + 0.6 * up[i] for i in range(3)]
                cam_rot = rec.set_camera(eye, target)
                model = rec.set_model(rot, pos)
                cid = f'{car}/{track}/{key.split(",")[0]}/{e}'
                rows.append(('case', cid, '-', f'light={lw["state"]};mirror={lw["mirror"]};'
                             f'env_slots={ev["slots"]};k={ev["k"]};inv_size={ev["inv_size"]};aspect={ev["aspect"]};'
                             f'env_pos={ev["pos"]};frame={ev["frame"]};o={ev["o"]};off={ev["off"]};'
                             f'model={",".join(f"{w:08x}" for w in model)};'
                             f'eye={",".join(bits(v) for v in eye)};cam={",".join(bits(v) for v in cam_rot)};'
                             f'near={bits(NEAR)};far={bits(FAR)}'))
                n_groups = 0
                for a in drawn:
                    part = rec.alloc(0x200)
                    rec.m.put_u32(part, CRP + a['entry']['off'])
                    rec.m.put_u32(part + 4, rec.m.call(CHUNK_FIND, 0xffffffff, ecx=CRP + a['entry']['off']))
                    for k in (k for k in a['kids'] if k['tag'] == b'pr' and 4096 <= k['idx'] < 8192):
                        mat = struct.unpack_from('<I', data, k['data'] + 4)[0] & 0xffff
                        mt = mts.get(mat)
                        mname = data[mt['data'] + 16:mt['data'] + 32].split(b'\0')[0].decode('latin-1') if mt else '?'
                        mflags = struct.unpack_from('<I', data, mt['data'] + 12)[0] if mt else 0
                        poly, trace, passes, sources, events = rec.draw_group(part, CRP + k['data'], k['count'], mat)
                        gid = f'{cid}/{a["name"]}/{k["idx"]}'
                        rows.append(('group', gid, '-', f'material={mat};method={mname};mt_flags={mflags:x};'
                                     f'poly={poly};count={k["count"]};'
                                     f'layers={"|".join(":".join(str(x) if not isinstance(x, int) else f"{x:x}" for x in t) for t in trace)}'))
                        rows.append(('param', gid, 'light', sources.pop('light_param')))
                        rows.append(('param', gid, 'sky_indexed', sources.pop('sky_indexed')))
                        for name, text in sources.items():
                            rows.append(('source', gid, name, text))
                        for j, p in enumerate(passes):
                            st = p['stride']
                            sl_ = p['slots']
                            slots = '|'.join(sl_[i * st:i * st + 32].hex() for i in range(len(sl_) // st))
                            rows.append(('pass', gid, j, f'flags={p["flags"]:x};stride={st:x};skip={p["skips"]};'
                                         f'slots={slots}'))
                        for j, evt in enumerate(events):
                            rows.append(('d3d', gid, j, event_text(evt)))
                        n_groups += 1
                print(f'{cid}: {n_groups} groups', flush=True)
    return rows


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--game-dir', type=Path, default=rh.ROOT / 'local/game')
    ap.add_argument('--fixtures', type=Path, default=rh.ROOT / 'local/fixtures',
                    help='directory with car-light-tracks.tsv and car-env-map-tracks.tsv')
    ap.add_argument('--out', type=Path, required=True)
    args = ap.parse_args()
    rows = record(args.game_dir, args.fixtures)
    lines = ['# replay-car-draw.py: nfs5.exe 0x44a770 scene, 0x44a250 + 0x53f1b0 method draw -> dx7z.dll -> '
             'D3D7 calls; GameData/CarModel CRPs and the light states of car-light-tracks.tsv / '
             'car-env-map-tracks.tsv (game data: keep in local/fixtures); f32 as bits, slots and vertex '
             'buffers as bytes', 'kind\tcase\tkey\tout']
    lines += ['\t'.join(str(c) for c in r) for r in rows]
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{args.out}: {len(rows)} rows')


if __name__ == '__main__':
    main()

"""Record the original MULTI* sky-map layer build: `SkyMapLayer_Ctor` 0x45b5a0 on synthetic
`Sky/<track>.txt` blocks, synthetic 16x16 maps and a synthetic sky texture pack, plus the
UV drift update and the driver's quad split.

Executes nfs5.exe and Drivers/dx7/dx7z.dll in Unicorn (render_harness.RenderMachine) on
synthetic inputs only (no game data is read; the fixture can be committed):

- `layer` / `vertex` / `dome` rows: 0x45b5a0 (thiscall, `ret 0x14`: cursor, pack, track,
  blend mode, alt-dome flag) runs for real with every callee: the token reader 0x45fae0, the
  CRT `atof` 0x5a21b8 / `strncmp` 0x5a2210, the pack lookup 0x460c60 -> 0x460b50 -> the SHPI
  directory search 0x53c680 (a synthetic SHPI in memory), the map objects 0x45e640 and
  0x45dd90 (texel copy, region A/B mix 0x45e160, 200 sprite records from the game RNG
  0x5561d0 -> 0x538040), the opacity test 0x45e110 -> 0x45e5e0, the cloud format test
  0x460d40 -> 0x556690, the material blocks 0x5379f0 / 0x537a50 / 0x537a40, the dome
  0x460ec0 -> 0x460f40 (SinTurns/CosTurns = x87 fsin/fcos), the quad lists 0x460d70 and the
  UV grid 0x45e8e0 (-> 0x45f1b0 scales and wind step, 0x45f3c0 static warp, 0x45ec40 UVs),
  the mesh 0x468310 and the channel setup 0x468630 -> 0x468440 (channel descriptors 0x53f230
  -> 0x53f980, 0x53f160, 0x53f2a0 -> 0x53f380; texture state 0x537a70/0x537a40/0x44dfe0/
  0x537a30/0x536470). The blank `flat` texture of the pack comes from the real pack ctor
  0x460a50 -> 0x535ed0.
  Recorded per layer: cloud resolution (which pack entry was turned into a texture, `flat` =
  pack[0]), the material block (0x540660 fields), the cloud texture's state block (+0x38),
  the mesh channel table (pointers resolved to named buffers), per vertex the colour bytes
  the colour channel points at and the UV floats, and the dome vertex positions.
- `mix` rows: the map object's mix 0x45e160 (thiscall, f32 t) for several t.
- `drift` rows: the UV-grid update 0x45f0a0 (vtable 0x5b3f28 slot 0, thiscall: camera*,
  f32 blend, int ticks) called repeatedly: layer offsets (+0x40/+0x44), camera offsets
  (+0x28/+0x2c) and the UVs of four vertices after each call.
- `quad` rows: the driver side of a quad draw. The layer material block goes through
  `Render_ApplyLayerState` 0x540660 and THRASH_setstate (as replay-render-state.py), then
  nfs5's quad-list emitter 0x57e000 (count, 32-byte vertices, 4-dword index quads, clip
  codes; unclipped quads -> 0x57e190 -> 0x57d7c0 -> [0x6bd9a8] = THRASH_drawquad@16) runs
  into dx7z.dll (back-face test, the quad batcher 0x60007af0); the driver batch is flushed
  (dx7z 0x60006f20) to the logging D3D device: DrawIndexedPrimitive arguments, the 16-bit
  index list and the diffuse dword of each vertex in the vertex buffer.
  Not reached: the path from the layer draw (0x45bd40 -> mesh 0x468410 -> vtable 0x5b3c94
  slot 1 0x53f1b0, the channel descriptors and the transform/clip code at [0x69e0d4]) to the
  quad list of 0x57e000; it would need the whole render context.

Harness services, not game code (stubs):
- the heap: 0x59ef90 / 0x59eff0 bump allocator (zeroed blocks), 0x59f050 free (no-op);
- texture creation 0x44da10 (cdecl: image entry, tag, ...): records the image entry and the
  tag and returns a fresh 0x54-byte texture object whose state block (+0x38) is initialised
  by the real 0x5379d0 (+4 holds the image entry as a harness marker); no image conversion
  (0x44d7e0) or surface (0x535bc0) is made. The pack's blank `flat` texture is the real
  0x535ed0 object of 0x460a50.
Start-up code run once per case: 0x449b40 (registers the game's vertex-channel factory
0x449b70 in front of 0x53f980 via 0x53f0b0, as the game does at start-up).
Globals set by hand: [0x628130] -> a render object whose +0x74 points at a texture-list
object (the `tex%d` counter at +0x10), [0x69ed0c] -> a view whose current textures are
none (0x536470 then only stores), the dx7z globals of replay-render-state.py, the driver's
batch buffers ([0x60017108] vertices, [0x60017114] indices, stride [0x600170fc] = 0x20,
capacity [0x60017100]) and cull mode [0x60058e1c] = 1 (none).
Every case starts from the pristine images (nfs5 .data incl. the RNG state, dx7z
.data/.bss) and an empty heap.

    python scripts/research/replay-sky-layer.py \
        --out iterations/012-campaign-fidelity/crates/nfs-assets/tests/fixtures/sky-layer.tsv
"""
import argparse
from pathlib import Path
import random
import struct
import sys


sys.path.insert(0, str(Path(__file__).resolve().parent))
import render_harness as rh  # noqa: E402
from render_harness import DATA  # noqa: E402

CTOR, MIX, UPDATE = 0x45b5a0, 0x45e160, 0x45f0a0
PACK_CTOR, TEX_BASE_INIT = 0x460a50, 0x5379d0
QUADS_EMIT, DRIVER_FLUSH = 0x57e000, 0x60006f20
APPLY_LAYER_STATE = 0x540660
CHANNEL_INIT = 0x449b40
MALLOC, NEW, FREE, TEX_CREATE = 0x59ef90, 0x59eff0, 0x59f050, 0x44da10
RENDER_OBJ_PTR, VIEW_PTR, RENDER_PTR = 0x628130, 0x69ed0c, 0x69e0d4
THRASH_DRAWQUAD_PTR = 0x6bd9a8
CACHE_VALUES = 0x5ded34
HEAP, HEAP_SIZE = 0x3000000, 0x400000
# Synthetic objects (DATA region of the harness).
TRACK, SKYBIN, RENDER_OBJ, TEXLIST, VIEW_P, VIEW_Q, VIEW = (
    DATA + 0x0, DATA + 0x1000, DATA + 0x9000, DATA + 0x9100, DATA + 0x9200, DATA + 0x9210,
    DATA + 0x9300)
CURSOR, TEXT, SHPI, PACK = DATA + 0x9400, DATA + 0x9500, DATA + 0xa000, DATA + 0x8000
CAMPOS, QVERTS, QINDEX, QCLIP, RENDER = (DATA + 0x9480, DATA + 0x1a000, DATA + 0x1b000,
                                         DATA + 0x1b800, DATA + 0x1c000)
DVBUF, DIBUF = DATA + 0x2e000, DATA + 0x2f000
DRIVER_SCENE, DRIVER_PENDING = 0x6001822c, 0x60058874


def bits(x):
    return format(struct.unpack('<I', struct.pack('<f', x))[0], '08x')


def u32f(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


class Recorder:
    def __init__(self, game_dir):
        self.m = m = rh.RenderMachine(game_dir)
        m.uc.mem_map(HEAP, HEAP_SIZE)
        self.setstate = m.exports['_THRASH_setstate@8']
        self.getstate = m.exports['_THRASH_getstate@4']
        self.drawquad = m.exports['_THRASH_drawquad@16']
        self.created = []

        def alloc(m_, args):
            size = max(1, args[0])
            p = self.heap
            self.heap = (self.heap + size + 15) & ~15
            if self.heap > HEAP + HEAP_SIZE:
                raise RuntimeError('heap exhausted')
            m.put_bytes(p, bytes(size))
            return p

        def tex_create(m_, args):
            entry, tag = args[0], args[1]
            obj = alloc(m_, [0x54])
            m.call(TEX_BASE_INIT, ecx=obj + 0x38)
            m.put_u32(obj + 4, entry)          # harness marker: the image the texture is made of
            self.created.append((entry, tag, obj))
            return obj

        for va, fn in ((MALLOC, alloc), (NEW, alloc), (FREE, lambda m_, a: 0), (TEX_CREATE, tex_create)):
            def handler(m_, _args, fn=fn):
                from unicorn.x86_const import UC_X86_REG_ESP
                sp = m.uc.reg_read(UC_X86_REG_ESP)
                a = list(struct.unpack('<5I', m.get_bytes(sp + 4, 20)))
                return fn(m_, a)
            m.stubs[va] = ('stub', 0, handler)
        # Pristine writable images (nfs5 .data/.bss, dx7z .data/.bss).
        self.pristine = {lo: m.get_bytes(lo, hi - lo) for lo, hi in
                         ((0x5cb000, 0x6c2000), (0x60017000, 0x600fd000))}

    # -- setup -----------------------------------------------------------------
    def reset(self):
        m = self.m
        for lo, data in self.pristine.items():
            m.put_bytes(lo, data)
        m.put_bytes(DATA, bytes(rh.DATA_SIZE - 0x10000))   # keep the fake D3D device (+0x30000)
        self.heap = HEAP
        self.created = []
        m.put_u32(RENDER_OBJ_PTR, RENDER_OBJ)
        m.put_u32(RENDER_OBJ + 0x74, TEXLIST)
        m.put_u32(VIEW_PTR, VIEW_P)
        m.put_u32(VIEW_P + 4, VIEW_Q)       # [[0x69ed0c]+4] +8 / +0xc: current textures (none)
        m.put_u32(VIEW_P, VIEW_Q)
        m.put_u32(VIEW_Q + 4, VIEW)         # 0x540590 reads [[[0x69ed0c]]+4]+0x10 (cull)
        m.put_i32(VIEW + 0x10, 2)
        m.put_u32(RENDER_PTR, RENDER)
        # Start-up registration of the game's vertex-channel factory (0x449b70, ids 0x8000+)
        # in front of 0x53f980: the real 0x449b40 -> 0x53f0b0.
        m.call(CHANNEL_INIT)

    def shpi(self, images):
        """Synthetic SHPI pack: images = [(name, code, width, height, pixel bytes)]."""
        n = len(images)
        head = bytearray(b'SHPI' + struct.pack('<II', 0, n) + b'GIMX')
        body = bytearray()
        offset = 16 + 8 * n
        dirs = bytearray()
        for name, code, w, h, px in images:
            dirs += name.encode().ljust(4, b'\0')[:4] + struct.pack('<I', offset + len(body))
            rec = struct.pack('<I', code | ((16 + len(px)) << 8)) + struct.pack('<HHHHHH', w, h, 0, 0, 0, 0)
            body += rec + px
            while len(body) % 16:
                body += b'\0'
        data = head + dirs + body
        struct.pack_into('<I', data, 4, len(data))
        self.m.put_bytes(SHPI, data)
        return SHPI

    def sky_bin(self, dome, alt=None):
        m = self.m
        m.put_u32(TRACK + 0x104, SKYBIN)
        angle, size, curvature, height, scale = dome
        for off, v in ((0x3c, angle), (0x40, size), (0x44, curvature), (0x48, height), (0xce4, scale)):
            m.put_bytes(SKYBIN + off, struct.pack('<f', v))
        if alt:
            for off, v in zip((0xb3c, 0xb40, 0xb44, 0xb48), alt):
                m.put_bytes(SKYBIN + off, struct.pack('<f', v))

    def text(self, tokens, flags):
        data = b''.join(t.lower().encode() + b'\0' for t in tokens) + b'end\0' + bytes(16)
        self.m.put_bytes(TEXT, data)
        self.m.put_bytes(CURSOR, struct.pack('<III', flags, TEXT, TEXT))

    # -- runs ------------------------------------------------------------------
    def build(self, case):
        """Run the pack ctor and 0x45b5a0 for one case; returns the layer object."""
        m = self.m
        self.reset()
        self.sky_bin(case['dome'], case.get('alt'))
        shpi = self.shpi(case['images'])
        m.call(PACK_CTOR, shpi, ecx=PACK)
        self.text(case['tokens'], case['flags'])
        layer = self.heap
        self.heap += 0x90
        m.put_bytes(layer, bytes(0x90))
        m.call(CTOR, CURSOR, PACK, TRACK, case['mode'], case.get('alt_flag', 0), ecx=layer, budget=20_000_000)
        return layer

    def names(self, layer):
        """Pointer -> label of the buffers the channels may point at."""
        m = self.m
        g = m.get_u32
        out = {}
        dome = g(layer + 0x30)
        out[g(dome + 4)] = 'dome.vertices'
        grid = g(layer + 0x40)
        for i in range(3):
            if g(grid + 4 + 4 * i):
                out[g(grid + 4 + 4 * i)] = f'uv{i}'
        out[g(grid + 0x18)] = 'grid.quads'
        out[g(g(layer + 0x44) + 4)] = 'cells.quads'
        for i in range(3):
            mo = g(layer + 0x34 + 4 * i)
            if mo:
                out[g(mo + 0x14)] = f'map{i}.texels'
            mat = g(layer + 0x70 + 4 * i)
            if mat:
                out[mat] = f'material{i}'
            tex = g(layer + 0x64 + 4 * i)
            if tex:
                out.setdefault(tex, f'cloud{i}')
        return out

    def layer_rows(self, cid, case, layer):
        m = self.m
        g = m.get_u32
        rows = []
        n_maps, n_layers = m.get_i32(layer + 0x7c), m.get_i32(layer + 0x80)
        flags = g(layer + 4)
        names = self.names(layer)
        pack_names = {}
        for k in range(40):
            e = g(PACK + 8 + 4 * k)
            if e:
                pack_names[e] = m.get_bytes(PACK + 0xa8 + 6 * k, 6).split(b'\0')[0].decode()
        flat = g(PACK)
        created = {obj: (entry, tag) for entry, tag, obj in self.created}
        mesh = g(layer + 0x60)
        per = m.get_bytes(mesh + 0x24, 1)[0]
        nchan = per * n_layers + m.get_bytes(mesh + 0x25, 1)[0]
        table = g(mesh + 4)
        channels = []
        for k in range(nchan):
            size, ptr = struct.unpack('<iI', m.get_bytes(table + 8 * k, 8))
            channels.append(f'{size}:{names.get(ptr, "0" if ptr == 0 else "?")}')
        slots = ','.join(str(b) for b in m.get_bytes(mesh + 0x1c, 10))
        rows.append(('layer', cid, '-', f'flags={flags:x};maps={n_maps};layers={n_layers};'
                     f'mesh_slots={slots};channels={"|".join(channels)}'))
        for i in range(3):
            tex = g(layer + 0x64 + 4 * i)
            if tex == 0:
                cloud = 'none'
            elif tex == flat:
                cloud = 'flat:pack0'
            elif tex in created:
                entry, tag = created[tex]
                cloud = f'image:{pack_names.get(entry, "?")}:tag={tag:08x}'
            else:
                cloud = '?'
            mat = g(layer + 0x70 + 4 * i)
            matb = ','.join(str(v) for v in struct.unpack('<6i', m.get_bytes(mat, 24))) if mat else '-'
            state = ','.join(str(v) for v in struct.unpack('<7i', m.get_bytes(tex + 0x38, 28))) if tex else '-'
            mo = g(layer + 0x34 + 4 * i)
            rows.append(('cloud', cid, i, f'{cloud};material={matb};texstate={state};mapobj={"yes" if mo else "no"}'))
        # Vertex colour channel (descriptor id 6 at slot +0x20) and UV channel (id 3, slot +0x1d).
        slot_col, slot_uv = m.get_bytes(mesh + 0x20, 1)[0], m.get_bytes(mesh + 0x1d, 1)[0]
        dome = g(layer + 0x30)
        verts = g(dome + 4)
        for i in range(n_layers):
            csize, cptr = struct.unpack('<iI', m.get_bytes(table + 8 * (per * i + slot_col), 8))
            usize, uptr = struct.unpack('<iI', m.get_bytes(table + 8 * (per * i + slot_uv), 8))
            colours = m.get_bytes(cptr, 256 * 4) if cptr else bytes(256 * 4)
            uv = m.get_bytes(uptr, 256 * 8)
            for v in range(256):
                cu, cv = struct.unpack_from('<II', uv, 8 * v)
                rows.append(('vertex', cid, f'{i}.{v}', f'{colours[4 * v:4 * v + 4].hex()};{cu:08x};{cv:08x}'))
        for v in range(256):
            x, y, z, w = struct.unpack('<4I', m.get_bytes(verts + 16 * v, 16))
            rows.append(('dome', cid, v, f'{x:08x},{y:08x},{z:08x},{w:08x}'))
        quads = m.get_bytes(g(g(layer + 0x40) + 0x18), 225 * 4)
        cells = m.get_bytes(g(g(layer + 0x44) + 4), 225 * 4)
        rows.append(('quads', cid, 'grid', quads.hex()))
        rows.append(('quads', cid, 'cells', cells.hex()))
        return rows

    def mix_rows(self, cid, layer):
        m = self.m
        rows = []
        for i in range(m.get_i32(layer + 0x7c)):
            mo = m.get_u32(layer + 0x34 + 4 * i)
            for t in (0.0, 0.25, 0.3, 0.5, 0.7, 0.999, 1.0):
                m.call(MIX, u32f(t), ecx=mo)
                rows.append(('mix', cid, f'{i}@{bits(t)}', m.get_bytes(m.get_u32(mo + 0x14), 1024).hex()))
        return rows

    def drift_rows(self, cid, layer, calls):
        m = self.m
        grid = m.get_u32(layer + 0x40)
        rows = []
        n = m.get_i32(grid + 0x20)
        for k, (cam, blend, ticks) in enumerate(calls):
            m.put_bytes(CAMPOS, struct.pack('<3f', *cam))
            m.call(UPDATE, CAMPOS, u32f(blend), ticks, ecx=grid)
            for i in range(n):
                off = struct.unpack('<2I', m.get_bytes(grid + 0x40 + 8 * i, 8))
                camoff = struct.unpack('<2I', m.get_bytes(grid + 0x28 + 8 * i, 8))
                uvp = m.get_u32(grid + 4 + 4 * i)
                uvs = [struct.unpack('<2I', m.get_bytes(uvp + 8 * v, 8)) for v in (0, 1, 17, 255)]
                rows.append(('drift', cid, f'{k}.{i}',
                             f'cam={",".join(bits(c) for c in cam)};blend={bits(blend)};ticks={ticks};'
                             f'offset={off[0]:08x},{off[1]:08x};camoff={camoff[0]:08x},{camoff[1]:08x};'
                             f'uv={"|".join(f"{a:08x},{b:08x}" for a, b in uvs)}'))
        return rows

    def step_rows(self, cid, layer):
        """Wind step and move flag as 0x45f1b0 left them (+0xa4/+0xa8 per layer, +0x70+i)."""
        m = self.m
        grid = m.get_u32(layer + 0x40)
        rows = []
        for i in range(m.get_i32(grid + 0x20)):
            su, sv = struct.unpack('<2I', m.get_bytes(grid + 0xa4 + 0x18 * i, 8))
            iu, iv = struct.unpack('<2I', m.get_bytes(grid + 0xec + 0x18 * i, 8))
            move = m.get_bytes(grid + 0x70 + i, 1)[0]
            rows.append(('step', cid, i, f'step={su:08x},{sv:08x};inv={iu:08x},{iv:08x};move={move};'
                         f'camflag={m.get_bytes(grid + 0x73, 1)[0]};warpflag={m.get_bytes(grid + 0x74, 1)[0]}'))
        return rows

    # -- driver quad split ---------------------------------------------------
    def quad_rows(self, block, colours):
        """Apply a layer material block, draw 4 synthetic quads through 0x57e000, flush the
        driver batch; returns the row of the D3D draw call and render states."""
        m = self.m
        self.reset()
        m.put_u32(rh.THRASH_SETSTATE_PTR, self.setstate)
        m.put_u32(rh.THRASH_GETSTATE_PTR, self.getstate)
        m.put_u32(THRASH_DRAWQUAD_PTR, self.drawquad)
        m.put_u32(rh.DRIVER_DEVICE, m.device)
        m.put_u32(DRIVER_SCENE, 1)
        m.put_u32(DRIVER_PENDING, 0)
        m.put_u32(0x60058878, 0)
        m.put_u32(0x60017108, DVBUF)
        m.put_u32(0x60017114, DIBUF)
        m.put_u32(0x600170fc, 0x20)
        m.put_u32(0x60017100, 0x80)
        m.put_u32(0x60058e1c, 1)
        for k in range(6):
            m.put_i32(CACHE_VALUES + 8 * k, 0x7fffffff)
        m.put_bytes(RENDER + 0x117e4, bytes(4))
        m.put_bytes(DATA + 0x20000, struct.pack('<6i', *block))
        m.log = []
        m.call(APPLY_LAYER_STATE, DATA + 0x20000)
        states = [f'rs{a[0]}={a[1]}' for name, a in m.log if name == 'SetRenderState']
        # 3x3 grid vertices (x, y, z, rhw, diffuse, specular, u, v); quads like FUN_0045e8e0.
        verts = b''
        for r in range(3):
            for c in range(3):
                verts += struct.pack('<4f', 10.0 + 40 * c, 10.0 + 30 * r, 0.5, 1.0)
                verts += struct.pack('<II2f', colours[3 * r + c], 0xff000000, 0.25 * c, 0.25 * r)
        m.put_bytes(QVERTS, verts)
        quads = [(0, 1, 4, 3), (1, 2, 5, 4), (3, 4, 7, 6), (4, 5, 8, 7)]
        m.put_bytes(QINDEX, b''.join(struct.pack('<4I', *q) for q in quads))
        m.put_bytes(QCLIP, bytes(16))
        m.put_bytes(0x5e1a00, bytes([0xff]))
        m.log = []
        m.call(QUADS_EMIT, len(quads), QVERTS, QINDEX, QCLIP)
        m.call(DRIVER_FLUSH)
        return [('quad', 'emit', ','.join(map(str, block)), self._draws() + ';' + ';'.join(states))]

    def _draws(self):
        m = self.m
        out = []
        for name, a in m.log:
            if name == 'DrawIndexedPrimitive':
                ptype, fvf, vptr, nv, iptr, ni = a[:6]
                idx = struct.unpack(f'<{ni}H', m.get_bytes(iptr, 2 * ni))
                diffuse = [m.get_u32(vptr + 0x20 * k + 0x10) for k in range(nv)]
                out.append(f'dip:type={ptype};fvf={fvf:x};nv={nv};idx={",".join(map(str, idx))};'
                           f'diffuse={",".join(f"{d:08x}" for d in diffuse)}')
            elif name not in ('SetRenderState', 'SetTextureStageState'):
                out.append(name)
        return ';'.join(out) or '-'


# -- synthetic inputs ------------------------------------------------------------
def map_pixels(kind, rnd):
    """16x16 BGRA texel bytes."""
    px = bytearray()
    for y in range(16):
        for x in range(16):
            if kind == 'ramp':
                b, g, r, a = x * 16, y * 16, (x * 7 + y * 3) & 0xff, 0xff
            elif kind == 'alpha':
                b, g, r, a = 0x80, y * 16, x * 16, (x * y) & 0xff
            elif kind == 'glow':
                d = (x - 10.4) ** 2 + (y - 5.5) ** 2
                v = max(0, 255 - int(d * 6))
                b, g, r, a = v // 2, v, 255, 0xff
            else:
                b, g, r, a = (rnd.randrange(256) for _ in range(4))
            px += bytes((b, g, r, a))
    return bytes(px)


def region(maps, xs, ys, ratio, speeds, dirs, warp_speed, warp_amount):
    t = list(maps) + [repr(v) for v in xs] + [repr(v) for v in ys] + [repr(ratio)]
    t += [repr(v) for v in speeds] + [repr(v) for v in dirs] + [repr(warp_speed), repr(warp_amount)]
    return t


def cases():
    rnd = random.Random(0x45b5a0)
    images = [('m1aa', 0x7d, 16, 16, map_pixels('ramp', rnd)),
              ('m2aa', 0x7d, 16, 16, map_pixels('random', rnd)),
              ('m3aa', 0x7d, 16, 16, map_pixels('alpha', rnd)),
              ('m4aa', 0x7d, 16, 16, map_pixels('glow', rnd)),
              ('m5aa', 0x7d, 16, 16, map_pixels('random', rnd)),
              ('c1aa', 0x7d, 8, 8, bytes(rnd.randrange(256) for _ in range(8 * 8 * 4))),
              ('c2aa', 0x78, 8, 8, bytes(rnd.randrange(256) for _ in range(8 * 8 * 2))),
              ('c3aa', 0x7f, 4, 4, bytes(rnd.randrange(256) for _ in range(4 * 4 * 3)))]
    dome_a = (0.62, 1500.0, 0.0002, 44.4, 0.236)
    dome_b = (0.0, 1000.0, 0.0002, 125.0, 1.5)
    out = []

    def case(name, sun, ra, rb, clouds, dome=dome_a, mode=0, flags=4, **kw):
        out.append(dict(name=name, tokens=[sun] + ra + rb + clouds, images=images, dome=dome,
                        mode=mode, flags=flags, **kw))
    # Coastal-like: one map, flat cloud, scales 2/6.
    r = region(['m4aa', 'none', 'none'], [2, 5, 0], [6, 1, 0], 0, [0, 6, 0], [90, 0, 0], 0, 0)
    case('one-map-flat', 'no', r, r, ['flat', 'none', 'none'])
    # Alps-like: two maps, flat + image cloud, region B different maps.
    ra = region(['m1aa', 'm3aa', 'none'], [1, 2, 0], [1, 2, 0], 0, [0, 2, 0], [90, 40, 0], 0, 0)
    rb = region(['m2aa', 'm5aa', 'none'], [1, 3, 0], [1.5, 4, 0], 0, [0, 5, 0], [90, 200, 0], 0, 0)
    case('two-maps-cloud', 'no', ra, rb, ['flat', 'c1aa', 'none'], dome=dome_b, blend=True)
    # Three layers, odd scales, every cloud kind (32-bit, 16-bit 565, 24-bit), blend modes 1..3.
    ra = region(['m2aa', 'm1aa', 'm3aa'], [0.5, 1.5, 3], [0.75, 4, 0.3], 1, [3, 10, 7], [90, 270, 220], 0, 0)
    for mode, flags in ((1, 8), (2, 0x10), (3, 0x20)):
        case(f'three-mode{mode}', 'yes', ra, ra, ['c2aa', 'c1aa', 'c3aa'], mode=mode, flags=flags)
    # Opaque map with a 565 cloud (blend switched off?), alpha map with a 565 cloud.
    ra = region(['m1aa', 'm3aa', 'none'], [1, 1, 0], [1, 1, 0], 0, [0, 0, 0], [0, 0, 0], 0, 0)
    case('opaque-vs-alpha-565', 'no', ra, ra, ['c2aa', 'c2aa', 'none'])
    # Clouds 'none' in the middle, more maps than clouds.
    ra = region(['m1aa', 'm2aa', 'm3aa'], [2, 3, 4], [2, 3, 4], 0, [1, 2, 3], [10, 20, 30], 0, 0)
    case('cloud-gap', 'no', ra, ra, ['flat', 'none', 'c1aa'])
    # Warp (static UV jitter of 0x45f3c0) and camera offset (move ratio > 1).
    ra = region(['m1aa', 'm2aa', 'none'], [1, 1.5, 0], [1, 2, 0], 400, [0, 3, 0], [90, 90, 0], 0.2, 0.1)
    case('warp-move', 'no', ra, ra, ['flat', 'c1aa', 'none'])
    # Alternate dome set (flag 0x200).
    ra = region(['m5aa', 'none', 'none'], [1, 0, 0], [1, 0, 0], 0, [0, 0, 0], [0, 0, 0], 0, 0)
    case('alt-dome', 'no', ra, ra, ['flat', 'none', 'none'], alt_flag=1, alt=(0.25, 800.0, 0.0003, 60.0))
    # Wind sweep: every speed/direction pair on the cloud layer (drift rows).
    for speed, direction in ((0.25, 0), (0.3, 45), (2, 90), (6, 135), (10, 180), (3, 220), (7, 270),
                             (5, 359), (1, -90), (40, 33.3)):
        ra = region(['m1aa', 'm2aa', 'none'], [1, 2, 0], [1, 3, 0], 0, [0.0, speed, 0], [0, direction, 0], 0, 0)
        case(f'wind-{speed}-{direction}', 'no', ra, ra, ['flat', 'c1aa', 'none'], drift=True)
    return out


DRIFT_CALLS = [((0.0, 0.0, 0.0), 0.0, 1), ((0.0, 0.0, 0.0), 0.0, 7), ((0.0, 0.0, 0.0), 0.0, 128),
               ((0.0, 0.0, 0.0), 0.0, 1000), ((0.0, 0.0, 0.0), 0.0, 33333),
               ((0.0, 0.0, 0.0), 0.0, -50)]
BLEND_CALLS = [((0.0, 0.0, 0.0), 0.5, 1), ((0.0, 0.0, 0.0), 1.0, 3), ((0.0, 0.0, 0.0), 0.25, 0),
               ((0.0, 0.0, 0.0), 0.7, 11)]
CAM_CALLS = [((0.0, 0.0, 0.0), 0.0, 1), ((123.5, 4.0, -77.25), 0.0, 3), ((-4000.0, 9.0, 2500.5), 0.0, 3)]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--game-dir', type=Path, default=None)
    ap.add_argument('--out', type=Path, required=True)
    args = ap.parse_args()
    rec = Recorder(args.game_dir)
    rows = []
    for cid, case in enumerate(cases()):
        layer = rec.build(case)
        rows.append(('case', cid, case['name'], ' '.join(case['tokens']) +
                     f';mode={case["mode"]};flags={case["flags"]:x};dome={",".join(bits(v) for v in case["dome"])}'
                     + (f';alt={",".join(bits(v) for v in case["alt"])}' if case.get('alt') else '')))
        rows += rec.layer_rows(cid, case, layer)
        rows += rec.step_rows(cid, layer)
        if cid == 0:
            rows.append(('shpi', '-', '-', bytes(rec.m.get_bytes(SHPI, rec.m.get_u32(SHPI + 4))).hex()))
        if case.get('drift'):
            rows += rec.drift_rows(cid, layer, DRIFT_CALLS)
        elif case['name'] == 'warp-move':
            rows += rec.drift_rows(cid, layer, CAM_CALLS)
        elif case.get('blend'):
            rows += rec.drift_rows(cid, layer, DRIFT_CALLS[:2] + BLEND_CALLS)
        else:
            rows += rec.drift_rows(cid, layer, DRIFT_CALLS[:2])
        if case['name'] in ('two-maps-cloud', 'three-mode1'):
            rows += rec.mix_rows(cid, layer)
    colours = [0xff102030, 0x80405060, 0x00a0b0c0, 0xffff0000, 0x7f00ff00, 0x010000ff, 0xffffffff, 0x00000000,
               0x12345678]
    for block in ((1, 2, 0, 0, 0, -1), (1, 0, 0, 0, 0, -1), (1, 2, 1, 0, 0, -1), (1, 2, 2, 0, 0, -1),
                  (1, 2, 3, 0, 0, -1), (0, 2, 0, 0, 0, -1)):
        rows += [(k, '-', b, o) for k, _, b, o in rec.quad_rows(block, colours)]
    lines = ['# replay-sky-layer.py: nfs5.exe 0x45b5a0 (SkyMapLayer_Ctor), 0x45e160, 0x45f0a0, '
             '0x57e000 + dx7z.dll THRASH_drawquad (synthetic inputs; f32 as bits, colours as bytes in memory order)',
             'kind\tcase\tkey\tout']
    lines += ['\t'.join(str(c) for c in r) for r in rows]
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{args.out}: {len(rows)} rows')


if __name__ == '__main__':
    main()

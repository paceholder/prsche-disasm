"""Record the original MULTI* sky-map layer draw: `SkyMapLayer_Draw` 0x45bd40 from the built
layer down to the Direct3D 7 calls of Drivers/dx7/dx7z.dll, for a set of camera orientations.

Executes nfs5.exe and dx7z.dll in Unicorn (render_harness.RenderMachine plus the PACODE
section 0x5b0000 of nfs5.exe, the hand-written vector library the transform calls):

- The layer is built by the real `SkyMapLayer_Ctor` 0x45b5a0 as in replay-sky-layer.py (same
  harness services: bump heap, texture creation 0x44da10 recording the image entry).
- Camera as the game makes one: `Camera_Ctor` 0x4672f0 (view object 0x538d10, viewport
  0x538da0 on a 640x480 display), `Camera_Activate` 0x467230 (0x466f30 matrices, 0x5390d0
  THRASH_clip), then what `SkyManager_Draw` 0x45d4f0 does before its layer pass: near/far
  0.15 / 700 (0x467120), camera position (0, 0, 0) (0x466220), 0x466f30. The orientation
  (`camera + 0xe8`, rows right / up / forward) is set by hand from a yaw and pitch.
- The draw 0x45bd40 runs with every callee: per cell (9 cells of 5 x 5 quads) the wedge cull
  0x45abb0 (and 0x45aaf0 for a level camera), the index rows 0x4687f0 / 0x468830 /
  0x4687c0, the mesh draw 0x468410 -> vtable 0x5b3c94 slot 1 0x53f1b0 -> the channel
  descriptor chain (material 0x540660, textures 0x536240 -> 0x555440 -> THRASH_settexture,
  indexed colour / UV 0x4499e0 / 0x449910, transform 0x540bc0 -> 0x5385b0 and PACODE, clip
  and emit 0x545ad0 / 0x54c030) -> THRASH_drawtrimesh@12 -> dx7z.dll batch -> the logging
  D3D7 device. Recorded per camera: every SetRenderState / SetTextureStageState /
  SetTexture call and every DrawIndexedPrimitive with its index list and its 32-byte TL
  vertices (x, y, z, rhw, diffuse, specular, u, v) as they are in the vertex buffer at the
  call, and the wedge-cull result per cell.

Textures: the cloud images (texture objects of 0x44da10) get a resident texture-manager
record (`+8` -> record, `+0x10` -> a dx7z texture record whose `+0x34` surface is a marker
0x7e0000nn, nn = pack index), so 0x555440 -> 0x5554b0 (resident, nothing to upload) ->
THRASH_settexture -> dx7z 0x600088b0 run for real and SetTexture shows the marker. The `flat`
texture is the real blank object of the pack ctor 0x460a50 -> 0x535ed0.

Harness globals: the THRASH pointer table [0x6bd910..0x6bd9b8] filled with the dx7z exports
as 0x574fa0 does; [0x619784] / [0x619788] = 640 / 480; the display-size query of
[[0x69ed0c]] (vtable slot 6) answers 640 x 480; [0x69e0d4] = the real render context of
0x538a30; texture flags [0x5de6b8] = [0x69dd1c] = [0x69dd1d] = [0x69dd1e] = 1, texture
manager [0x6a3a48] = 1; driver: device, scene open, batch buffers as replay-sky-layer.py,
stage count [0x6005ab80] = 2.

Inputs: `--synthetic` the synthetic cases of replay-sky-layer.py (no game data; the fixture
can be committed); `--tracks` the HIGH block of GameData/Track/Sky/<track>.txt with the
track's own .fsh and .bin (game data: the fixture stays in local/fixtures).

    python scripts/research/replay-sky-draw.py --synthetic \
        --out iterations/012-campaign-fidelity/crates/nfs-assets/tests/fixtures/sky-draw.tsv
    python scripts/research/replay-sky-draw.py --game-dir local/game --tracks \
        --out local/fixtures/sky-draw-tracks.tsv
"""
import argparse
import importlib.util
import math
from pathlib import Path
import struct
import sys

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_ESP

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import render_harness as rh  # noqa: E402

_spec = importlib.util.spec_from_file_location('replay_sky_layer', HERE / 'replay-sky-layer.py')
sl = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(sl)

PACODE = (0x5b0000, 0x5b1af9)
LAYER_DRAW, CULL_RET = 0x45bd40, 0x45be89
CAMERA_CTOR, CAMERA_ACTIVATE, CAMERA_NEAR_FAR, CAMERA_POSITION, CAMERA_UPDATE = (
    0x4672f0, 0x467230, 0x467120, 0x466220, 0x466f30)
RENDER_CTX_CTOR = 0x538a30
NEAR, FAR = 0.15, 700.0
# THRASH pointer table of 0x574fa0 (GetProcAddress order) and its four aliases.
THRASH = {
    0x6bd934: '_THRASH_about@0', 0x6bd91c: '_THRASH_clearwindow@0', 0x6bd9b4: '_THRASH_clip@16',
    0x6bd950: '_THRASH_drawline@8', 0x6bd93c: '_THRASH_drawlinemesh@12', 0x6bd994: '_THRASH_drawlinestrip@12',
    0x6bd924: '_THRASH_drawpoint@4', 0x6bd960: '_THRASH_drawpointmesh@12', 0x6bd9a8: '_THRASH_drawquad@16',
    0x6bd944: '_THRASH_drawquadmesh@12', 0x6bd928: '_THRASH_drawtri@12', 0x6bd98c: '_THRASH_drawtrifan@12',
    0x6bd9ac: '_THRASH_drawtrimesh@12', 0x6bd99c: '_THRASH_drawtristrip@12', 0x6bd974: '_THRASH_drawsprite@8',
    0x6bd910: '_THRASH_drawspritemesh@12', 0x6bd970: '_THRASH_flushwindow@0', 0x6bd95c: '_THRASH_init@0',
    0x6bd990: '_THRASH_is@0', 0x6bd964: '_THRASH_lockwindow@0', 0x6bd948: '_THRASH_pageflip@0',
    0x6bd96c: '_THRASH_readrect@20', 0x6bd938: '_THRASH_restore@0', 0x6bd9b8: '_THRASH_selectdisplay@4',
    0x6bd97c: '_THRASH_setstate@8', 0x6bd984: '_THRASH_getstate@4', 0x6bd954: '_THRASH_settexture@4',
    0x6bd958: '_THRASH_setvideomode@12', 0x6bd978: '_THRASH_sync@4', 0x6bd968: '_THRASH_talloc@20',
    0x6bd94c: '_THRASH_tfree@4', 0x6bd988: '_THRASH_treset@0', 0x6bd940: '_THRASH_tupdate@12',
    0x6bd92c: '_THRASH_unlockwindow@4', 0x6bd9b0: '_THRASH_window@4', 0x6bd930: '_THRASH_writerect@20'}
THRASH_ALIASES = ((0x6bd9a0, 0x6bd984), (0x6bd998, 0x6bd984), (0x6bd918, 0x6bd97c), (0x6bd920, 0x6bd97c))
D = rh.DATA
DISPLAY_VT, DISPLAY, CTX_OWNER, POS, TEXREC = D + 0x24800, D + 0x24900, D + 0x24a00, D + 0x24b00, D + 0x25000
PACK_MEM, PACK_MEM_SIZE = 0x3800000, 0x100000
SURFACE_MARK = 0x7e000000
# Camera orientations (yaw, pitch in degrees; yaw about +y from +z towards +x).
CAMERAS = [(0, 0), (0, 10), (0, 30), (30, 60), (90, 5), (135, 20), (180, 3), (225, 45), (270, 0),
           (315, 80), (45, 89), (200, -10), (100, 15), (330, 2)]
SYNTHETIC_CAMERAS = [(0, 0), (0, 10), (90, 5), (135, 20), (225, 45), (45, 89), (200, -10)]


def bits(x):
    return sl.bits(x)


def orientation(yaw, pitch):
    cy, sy = math.cos(math.radians(yaw)), math.sin(math.radians(yaw))
    cp, sp = math.cos(math.radians(pitch)), math.sin(math.radians(pitch))
    forward = (sy * cp, sp, cy * cp)
    right = (cy, 0.0, -sy)
    up = (-sy * sp, cp, -cy * sp)
    return [struct.unpack('<f', struct.pack('<f', v))[0] for v in (*right, *up, *forward)]


class DrawRecorder(sl.Recorder):
    def __init__(self, game_dir):
        super().__init__(game_dir)
        m = self.m
        m.code.append(PACODE)
        m.uc.mem_map(PACK_MEM, PACK_MEM_SIZE)
        self.events = None
        self.culls = None

        def log_call(name):
            def handler(m_, args):
                if self.events is not None:
                    if name == 'DrawIndexedPrimitive':
                        ptype, fvf, vptr, nv, iptr, ni = args[1:7]
                        idx = struct.unpack(f'<{ni}H', m.get_bytes(iptr, 2 * ni))
                        self.events.append(('dip', (ptype, fvf, idx, m.get_bytes(vptr, 32 * nv))))
                    else:
                        self.events.append((name, tuple(args[1:])))
                m.log.append((name, tuple(args[1:])))
                return 0
            return handler
        for va, (name, pop, _h) in list(m.stubs.items()):
            if name in ('DrawIndexedPrimitive', 'SetRenderState', 'SetTextureStageState', 'SetTexture',
                        'DrawPrimitive', 'DrawPrimitiveVB', 'DrawIndexedPrimitiveVB'):
                m.stubs[va] = (name, pop, log_call(name))

        def display_size(m_, args):
            m.put_bytes(args[0], struct.pack('<f', 640.0))
            m.put_bytes(args[1], struct.pack('<f', 480.0))
            return 0
        self.display_stub = m.add_stub('display.size', 2, display_size)

        def on_cull(uc, _a, _s, _u):
            if self.culls is not None:
                cell = uc.reg_read(UC_X86_REG_EBP)
                self.culls[cell] = 'd' if uc.reg_read(UC_X86_REG_EAX) & 0xff else 'c'
        m.uc.hook_add(UC_HOOK_CODE, on_cull, begin=CULL_RET, end=CULL_RET)

        def on_cell(uc, _a, _s, _u):
            # 0x45be8d: the cell is drawn (reached with or without the wedge test).
            if self.culls is not None:
                cell = uc.reg_read(UC_X86_REG_EBP)
                if self.culls[cell] == '-':
                    self.culls[cell] = 'n'
        m.uc.hook_add(UC_HOOK_CODE, on_cell, begin=0x45be8d, end=0x45be8d)

    # -- layer -----------------------------------------------------------------
    def build_real(self, skybin, fsh, tokens, flags, mode=0):
        m = self.m
        self.reset()
        m.put_bytes(sl.SKYBIN, skybin)
        m.put_u32(sl.TRACK + 0x104, sl.SKYBIN)
        if len(fsh) > PACK_MEM_SIZE:
            raise RuntimeError('pack too large')
        m.put_bytes(PACK_MEM, fsh)
        m.call(sl.PACK_CTOR, PACK_MEM, ecx=sl.PACK)
        self.text(tokens, flags)
        layer = self.heap
        self.heap += 0x90
        m.put_bytes(layer, bytes(0x90))
        m.call(sl.CTOR, sl.CURSOR, sl.PACK, sl.TRACK, mode, 0, ecx=layer, budget=20_000_000)
        return layer

    # -- draw setup ------------------------------------------------------------
    def setup_draw(self):
        """Driver, THRASH table, render context and camera; returns the camera."""
        m = self.m
        g = m.get_u32
        for slot, name in THRASH.items():
            m.put_u32(slot, m.exports[name])
        for a, b in THRASH_ALIASES:
            m.put_u32(a, g(b))
        m.put_u32(rh.DRIVER_DEVICE, m.device)
        m.put_u32(sl.DRIVER_SCENE, 1)
        m.put_u32(sl.DRIVER_PENDING, 0)
        m.put_u32(0x60058878, 0)
        m.put_u32(0x60017108, sl.DVBUF)
        m.put_u32(0x60017114, sl.DIBUF)
        m.put_u32(0x600170fc, 0x20)
        m.put_u32(0x60017100, 0x80)
        m.put_u32(0x60058e1c, 1)
        m.put_u32(0x6005ab80, 2)
        for k in range(6):
            m.put_i32(sl.CACHE_VALUES + 8 * k, 0x7fffffff)
        m.put_u32(sl.VIEW_Q, DISPLAY_VT)
        m.put_u32(DISPLAY_VT + 0x18, self.display_stub)
        for va in (0x5de6b8, 0x69dd1c, 0x69dd1d, 0x69dd1e):
            m.put_bytes(va, b'\x01')
        m.put_u32(0x6a3a48, 1)
        m.put_u32(0x619784, 640)
        m.put_u32(0x619788, 480)
        m.call(RENDER_CTX_CTOR, ecx=CTX_OWNER)
        m.put_u32(sl.RENDER_OBJ + 0x70, DISPLAY)
        cam = m.call(CAMERA_CTOR, 4, sl.u32f(0.0), sl.u32f(0.0), sl.u32f(1.0), sl.u32f(1.0), 0,
                     sl.u32f(NEAR), sl.u32f(FAR), ecx=sl.RENDER_OBJ)
        # Resident texture records for the cloud images (see the module doc).
        for k, (entry, _tag, obj) in enumerate(self.created):
            rec, drv = TEXREC + 0x80 * k, TEXREC + 0x80 * k + 0x40
            m.put_bytes(rec, bytes(0x80))
            m.put_u32(rec + 0x10, drv)
            m.put_u32(rec + 0x2c, 2)
            m.put_u32(drv + 0x34, SURFACE_MARK | self.pack_index(entry))
            m.put_u32(obj + 8, rec)
        return cam

    def pack_index(self, entry):
        for k in range(40):
            if self.m.get_u32(sl.PACK + 8 + 4 * k) == entry:
                return k
        return 0xff

    def draw(self, layer, cam, rot):
        m = self.m
        # A frame as the game starts one: the camera (orientation at +0xe8, position at +0x10c)
        # is activated (0x467230: transform cache reset 0x44a150, matrices 0x466f30, view
        # 0x5390d0), then SkyManager_Draw 0x45d4f0 sets near/far 0.15/700 and moves the camera
        # to the origin before its layer pass.
        m.put_bytes(cam + 0xe8, struct.pack('<9f', *rot))
        m.put_bytes(cam + 0x10c, bytes(12))
        m.call(CAMERA_ACTIVATE, cam, ecx=sl.RENDER_OBJ)
        m.call(CAMERA_NEAR_FAR, sl.u32f(NEAR), sl.u32f(FAR), ecx=cam)
        m.put_bytes(POS, bytes(12))
        m.call(CAMERA_POSITION, POS, ecx=cam + 0xb8)
        m.call(CAMERA_UPDATE, ecx=cam)
        self.events, self.culls = [], ['-'] * 9
        m.call(LAYER_DRAW, 0, 0, ecx=layer, budget=200_000_000)
        m.call(sl.DRIVER_FLUSH)
        events, culls = self.events, ''.join(self.culls)
        self.events = self.culls = None
        return events, culls


def event_text(ev):
    name, a = ev
    if name == 'dip':
        ptype, fvf, idx, vb = a
        return (f'dip:type={ptype};fvf={fvf:x};idx={",".join(map(str, idx))};vb={vb.hex()}')
    if name == 'SetRenderState':
        return f'rs:{a[0]}={a[1]}'
    if name == 'SetTextureStageState':
        return f'tss:{a[0]},{a[1]}={a[2]}'
    if name == 'SetTexture':
        return f'tex:{a[0]}={a[1]:08x}'
    return f'{name}:{",".join(map(str, a))}'


def record_case(rec, cid, name, case_text, layer, case_for_rows, cameras, layer_rows):
    rows = [('case', cid, name, case_text)]
    if layer_rows:
        rows += rec.layer_rows(cid, case_for_rows, layer)
    cam = rec.setup_draw()
    for k, (yaw, pitch) in enumerate(cameras):
        rot = orientation(yaw, pitch)
        events, culls = rec.draw(layer, cam, rot)
        rows.append(('camera', cid, k, f'yaw={yaw};pitch={pitch};rot={",".join(bits(v) for v in rot)};'
                     f'near={bits(NEAR)};far={bits(FAR)};half_fov={rec.m.get_u32(cam + 0x130):08x};'
                     f'cull={culls}'))
        for j, ev in enumerate(events):
            rows.append(('draw', cid, f'{k}.{j}', event_text(ev)))
    return rows


def find(dirpath, name):
    for p in dirpath.iterdir():
        if p.name.lower() == name.lower():
            return p
    return None


def track_cases(game_dir):
    sky = game_dir / 'GameData/Track/Sky'
    out = []
    for track in ('Alps', 'Autobahn', 'Castle', 'Coastal', 'Farmland', 'Industrial', 'Skidpad', 'Forest',
                  'foothills'):
        txt, fsh, binf = find(sky, f'{track}.txt'), find(sky, f'{track}.fsh'), find(sky, f'{track}.bin')
        if not (txt and fsh and binf):
            continue
        toks = [ln.split('#')[0].strip() for ln in txt.read_text(encoding='latin-1').splitlines()]
        toks = [t.split()[0] for t in toks if t]
        i = toks.index('BEGIN')
        block = toks[i + 1:toks.index('END', i)]
        if block[0].upper() != 'HIGH' or not block[1].upper().startswith('MULTI'):
            continue
        flags = {'MULTINORMAL': 4, 'MULTIADDITIVE': 8}.get(block[1].upper(), 4)
        out.append(dict(name=track, tokens=block[2:2 + 1 + 36 + 3], flags=flags, mode=0,
                        skybin=binf.read_bytes(), fsh=fsh.read_bytes()))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--game-dir', type=Path, default=None)
    ap.add_argument('--out', type=Path, required=True)
    group = ap.add_mutually_exclusive_group(required=True)
    group.add_argument('--synthetic', action='store_true')
    group.add_argument('--tracks', action='store_true')
    args = ap.parse_args()
    rec = DrawRecorder(args.game_dir)
    rows = []
    if args.synthetic:
        # Case ids are those of replay-sky-layer.py: the layers (vertex colours, UVs, dome) are
        # the rows of its fixture sky-layer.tsv.
        keep = ('one-map-flat', 'two-maps-cloud', 'cloud-gap')
        for cid, case in enumerate(sl.cases()):
            if case['name'] not in keep:
                continue
            layer = rec.build(case)
            text = (' '.join(case['tokens']) + f';mode={case["mode"]};flags={case["flags"]:x};'
                    f'dome={",".join(bits(v) for v in case["dome"])}')
            rows += record_case(rec, cid, case['name'], text, layer, case, SYNTHETIC_CAMERAS, False)
        source = 'synthetic inputs (case ids and layers of replay-sky-layer.py / sky-layer.tsv)'
    else:
        game_dir = args.game_dir or rh.ROOT / 'local/game'
        for cid, case in enumerate(track_cases(game_dir)):
            layer = rec.build_real(case['skybin'], case['fsh'], case['tokens'], case['flags'], case['mode'])
            text = ' '.join(case['tokens']).lower() + f';mode={case["mode"]};flags={case["flags"]:x}'
            rows += record_case(rec, cid, case['name'], text, layer, case, CAMERAS, True)
        source = 'GameData/Track/Sky HIGH blocks (game data: keep in local/fixtures)'
    lines = ['# replay-sky-draw.py: nfs5.exe 0x45bd40 (SkyMapLayer_Draw) -> dx7z.dll -> D3D7 calls; '
             + source + '; f32 as bits, vertex buffers as bytes', 'kind\tcase\tkey\tout']
    lines += ['\t'.join(str(c) for c in r) for r in rows]
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{args.out}: {len(rows)} rows')


if __name__ == '__main__':
    main()

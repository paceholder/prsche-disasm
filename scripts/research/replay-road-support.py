#!/usr/bin/env python3
"""Record the original road support fixtures (change original-road-support;
docs/re/support.md in the workspace root).

Executes nfs5.exe in Unicorn (vehicle_harness.py, x87 control word 0x27f) on
real track support:

- Polygons: the type-1 support polygons of a track in insertion order, as
  the Rust port assembles them from the CRP (`examples/support_dump.rs`,
  `<track>-polygons.tsv`). Each is built by the original constructors
  (quad 0x4853c0, triangle 0x485c50; thiscall (vertex pointers, colours,
  flags word)) on its own vertex buffer.
- Tree: the retained support tree of `0x488090`: `0x483800` (thiscall,
  half extent 8192.0, 15 levels) and `0x484ae0` per polygon on the root
  node, the order of `0x483a90`. `[0x628960]+0x24` points at the tree, as
  `0x473240` reads it. EDG objects (type 2) are not inserted (they change
  only the leaf partition, never a selection; see support.md).
- Node graph `[0x628bc0]`: `TrackNodes_New` 0x47fd30 and `TrackNodes_Build`
  0x480140 on the CRP's SimD/SimT (ai_harness.py).

Harness services, not game code: the heap behind the game's allocators
(0x59ef90, 0x59eff0, 0x59ecb0, 0x4211d0 allocate; 0x59ecd0, 0x59f050,
0x4237d0, 0x531f90 free nothing), the node pool refill 0x59eeb0 (links
fresh 8·(bucket+1)-byte blocks into the pool's free list `[0x5e4fe8]+0x34`),
critical sections 0x5322b0/0x5322c0 (no-ops). Everything else runs as is.

Fixtures (support-*.tsv):
- query: owner method calls on real tracks. A stream is one owner
  (0x30 bytes, zeroed at the stream start) and a sequence of
  `0x474060(point, reuse)`, then `0x474420` (normal), `0x474430`
  (tangent), `0x4745f0(0..3)` (vertices). Streams: single queries at random
  interior points, exact vertices and edge midpoints (fresh owner each),
  random points in the track box, and walks along node centre lines with
  reuse 1 and 0.
- loop-runs / loop-body / loop-dyn: closed-loop drives of the 993 C4 (V6 car
  setup) on real tracks: on even steps `Car_TrackLocate` 0x49af70 (prio 6),
  the input word, `Car_ReadInputs` 0x4991e0, `Car_DynamicsTick` 0x49fd30,
  `Car_GroundContactTick` 0x49bd40 with the real support owners; then
  0x49b7a0, 0x49b8e0 on every step. The steering word comes from a simple
  controller that aims at the node centre 6 nodes ahead (only the words are
  replayed by the port). The overhead query 0x473330 returns none (A28), and
  runs reaching 0x496610 or 0x4998c0 stop with an error.

    python scripts/research/replay-road-support.py --game-dir local/game \\
        --polygons local/derived/support \\
        --out-dir local/fixtures
"""
import argparse
import importlib.util
import math
from pathlib import Path
import random
import struct
import sys

from unicorn import UC_HOOK_MEM_WRITE

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import vehicle_harness as vh  # noqa: E402
import ai_harness as ah  # noqa: E402

_spec = importlib.util.spec_from_file_location('vground', HERE / 'replay-vehicle-ground.py')
vg = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(vg)
vs = vg.vs
f32, word, short = vs.f32, vs.word, vs.short

HEAP, HEAP_SIZE = 0x3000000, 0x6000000
POOL = HEAP                      # pool object for [0x5e4fe8]
POOL_PTR = 0x5e4fe8
SCENE_PTR = 0x628960
TRACK_NODES_PTR = 0x628bc0

QUAD_NEW, TRI_NEW = 0x4853c0, 0x485c50
TREE_NEW, TREE_INSERT = 0x483800, 0x484ae0
SELECT, NORMAL, TANGENT, VERTEX = 0x474060, 0x474420, 0x474430, 0x4745f0
TRACK_LOCATE, NODE_STEP = 0x49af70, 0x4817d0
POLY_STRIDE = 0x40

ALLOCATORS = (0x59ef90, 0x59eff0, 0x59ecb0)
FREES = (0x59ecd0, 0x59f050, 0x4237d0, 0x531f90)
POOL_ALLOC = 0x4211d0
POOL_REFILL = 0x59eeb0

QOWNER = vh.SCRATCH + 0x2000    # owner of the query streams (0x30 bytes)
QPOINT = vh.SCRATCH + 0x2040
CAR = vh.CAR


def bits(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


def unbits(b):
    return struct.unpack('<f', struct.pack('<I', b))[0]


def load_polygons(path):
    out = []
    for line in path.read_text().splitlines()[1:]:
        i, n, v, flags = line.split('\t')
        vals = [unbits(int(h, 16)) for h in v.split(',')]
        pts = [tuple(vals[3 * k:3 * k + 3]) for k in range(int(n))]
        out.append((pts, int(flags)))
    return out


class SupportRecorder(vg.GroundRecorder):
    """The V6 ground recorder with the real support tree instead of the
    owner stubs."""

    def __init__(self, game_dir):
        module, data = vh.load_module('nfs5.exe', game_dir)
        stubs = {va: (lambda m: 0) for va in vs.STUBS_NOP}
        stubs[vs.CALLBACK_STUB] = lambda m: 0
        for va in ALLOCATORS:
            stubs[va] = lambda m: self.alloc(self.arg(0))
        stubs[POOL_ALLOC] = lambda m: self.alloc(self.arg(0))
        for va in FREES:
            stubs[va] = lambda m: 0
        stubs[POOL_REFILL] = self._refill
        stubs[vg.TRACK_QUERY] = vg.thiscall(2, lambda *_: 0)
        self.reached = set()
        stubs[vg.ROLLOVER] = self._unreachable(vg.ROLLOVER)
        stubs[vg.FLAG80_PATH] = self._unreachable(vg.FLAG80_PATH)
        stubs[ah.MATH_ERROR] = lambda m: m.uc.reg_read(ah.UC_X86_REG_EAX)
        self.m = m = vh.Machine(module, data, stubs=stubs, trace_calls=False)
        base = int(module['image_base'], 16)
        pacode = next(s for s in module['sections'] if s['name'] == 'PACODE')
        m.code = (m.code[0], base + pacode['rva'] + pacode['vsize'])
        m.uc.mem_write(ah.CPU_3DNOW, b'\0')
        m.uc.mem_write(ah.CPU_SSE, b'\0')
        m.uc.mem_map(HEAP, HEAP_SIZE)
        self.heap = HEAP + 0x1000
        # Pool object: +0x28 critical section, +0x34.. free lists by size/8.
        m.put_u32(POOL_PTR, POOL)
        self.written = set()
        self.tracking = False
        stack = (vh.STACK_TOP - 0x10000, vh.STACK_TOP)

        def on_write(_uc, _access, address, size, _value, _user):
            if self.tracking and not stack[0] <= address < stack[1]:
                self.written.update(range(address, address + size))

        m.uc.hook_add(UC_HOOK_MEM_WRITE, on_write)
        self.specs, self.fields = {}, {}
        for name, sim, model in vs.CARS:
            raw = (game_dir / 'GameData/Simulation/CarData' / sim).read_bytes()
            self.specs[name] = vh.derive_specs(m, raw, model)
            self.fields[name] = dict(vh.unpack_fields(self.specs[name], vh.CARSPECS_FIELDS))
        self.allowed = set(range(CAR, CAR + vh.CAR_SIZE))
        # 0x473240 counts queries and cache hits at 0x6289ec/0x6289f0; the
        # containment callbacks store their edge test in 0x6b0068.
        for addr, size in vs.GLOBAL_WRITES + [(0x6289ec, 8), (0x6b0068, 4)]:
            self.allowed |= set(range(addr, addr + size))

    # -- harness services -------------------------------------------------
    def arg(self, i):
        sp = self.m.uc.reg_read(vg.UC_X86_REG_ESP)
        return self.m.get_u32(sp + 4 + 4 * i)

    def alloc(self, size):
        size = max(int(size), 1)
        addr = self.heap
        self.heap = (self.heap + size + 15) & ~15
        if self.heap > HEAP + HEAP_SIZE:
            raise RuntimeError('harness heap exhausted')
        self.m.uc.mem_write(addr, bytes(size))
        return addr

    def _refill(self, m):
        bucket = self.arg(0)
        size = 8 * (bucket + 1)
        head = 0
        for _ in range(1024):
            block = self.alloc(size)
            m.put_u32(block + 4, head)
            head = block
        m.put_u32(POOL + 0x34 + 4 * bucket, head)
        return 0

    # -- scene ------------------------------------------------------------
    def build_support(self, polygons):
        m = self.m
        n = len(polygons)
        self.poly_base = self.alloc(n * POLY_STRIDE)
        verts = self.alloc(n * 0x40)
        ptrs = self.alloc(0x10)
        colours = self.alloc(0x10)
        for i, (pts, flags) in enumerate(polygons):
            vb = verts + i * 0x40
            for k, p in enumerate(pts):
                m.put(vb + 16 * k, *p, 0.0)
                m.put_u32(ptrs + 4 * k, vb + 16 * k)
            obj = self.poly_base + i * POLY_STRIDE
            m.call_thiscall(TRI_NEW if len(pts) == 3 else QUAD_NEW, obj, ptrs, colours, flags)
        tree = self.alloc(8)
        m.call_thiscall(TREE_NEW, tree, bits(8192.0), 15, budget=10_000_000)
        root = m.get_u32(tree)
        for i in range(n):
            m.call_thiscall(TREE_INSERT, root, self.poly_base + i * POLY_STRIDE, budget=50_000_000)
        scene = self.alloc(0x40)
        m.put_u32(scene + 0x24, tree)
        m.put_u32(SCENE_PTR, scene)
        self.npolys = n

    def build_nodes(self, crp):
        n, nodes, sections = ah.sim_nodes(crp)
        m = self.m
        for init in ah.TRACKNODES_STATIC_INIT:
            m.call(init, budget=1000)
        m.call(ah.TRACKNODES_NEW, budget=100_000)
        obj = m.get_u32(TRACK_NODES_PTR)
        na = self.alloc(len(nodes))
        m.uc.mem_write(na, nodes)
        sa = 0
        if sections:
            sa = self.alloc(4 * len(sections))
            m.uc.mem_write(sa, struct.pack(f'<{len(sections)}i', *sections))
        m.call_thiscall(ah.TRACKNODES_BUILD, obj, n, len(sections), na, sa, budget=400_000_000)
        self.nodes_obj = obj
        self.node_count = n
        self.node_array = m.get_u32(obj + 0xc)

    def node(self, k):
        a = self.node_array + k * 0x50
        g = self.m.get
        return {'pos': g(a + 4, 3), 'up': g(a + 0x10, 3), 'fwd': g(a + 0x1c, 3), 'lat': g(a + 0x28, 3)}

    def step_node(self, node, delta):
        r = self.m.call_thiscall(NODE_STEP, self.nodes_obj, node, delta & 0xffffffff, 0xffffffff)
        return struct.unpack('<i', struct.pack('<I', r))[0]

    def poly_index(self, ptr):
        if ptr == 0:
            return -1
        k, r = divmod(ptr - self.poly_base, POLY_STRIDE)
        if r or not 0 <= k < self.npolys:
            raise RuntimeError(f'owner polygon {ptr:#x} outside the arena')
        return k

    def node_code(self, ptr):
        return -1 if ptr == 0 else self.m.get_u32(ptr)

    # -- owner rows -------------------------------------------------------
    def owner_row(self, owner, prefix):
        m = self.m
        g = m.get_bytes
        row = [(f'{prefix}poly', self.poly_index(m.get_u32(owner + 0x28))),
               (f'{prefix}node', self.node_code(m.get_u32(owner + 0x2c))),
               (f'{prefix}b02', g(owner + 2, 1)[0]), (f'{prefix}b09', g(owner + 9, 1)[0]),
               (f'{prefix}b0a', g(owner + 0xa, 1)[0]),
               (f'{prefix}flags', struct.unpack('<H', g(owner + 0x24, 2))[0])]
        row += [(f'{prefix}n{k}', v) for k, v in enumerate(m.get(owner + 0xc, 3))]
        row += [(f'{prefix}t{k}', v) for k, v in enumerate(m.get(owner + 0x18, 3))]
        return row

    def query(self, point, reuse):
        """0x474060, 0x474420, 0x474430, 0x4745f0(0..3) on QOWNER."""
        m = self.m
        m.put(QPOINT, *point)
        m.call_thiscall(SELECT, QOWNER, QPOINT, reuse)
        after_select = self.owner_row(QOWNER, 'sel_')
        n = m.get(m.call_thiscall(NORMAL, QOWNER), 3)
        t = m.get(m.call_thiscall(TANGENT, QOWNER), 3)
        vs4 = [m.get(m.call_thiscall(VERTEX, QOWNER, k), 3) for k in range(4)]
        row = after_select + [(f'normal{k}', v) for k, v in enumerate(n)]
        row += [(f'tangent{k}', v) for k, v in enumerate(t)]
        row += [(f'v{j}_{k}', v) for j in range(4) for k, v in enumerate(vs4[j])]
        row += self.owner_row(QOWNER, 'out_')
        return row

    # -- locator rows -----------------------------------------------------
    LOCATE_FIELDS = [
        ('loc_node_08', 0x8, 'hs'), ('loc_section_0c', 0xc, 'i'), ('loc_b10', 0x10, 'b'),
        ('loc_b11', 0x11, 'b'), ('loc_b12', 0x12, 'b'), ('loc_b0a', 0xa, 'b'),
        ('body_flags_2c', 0x2c, 'h'), ('dist_78', 0x78, 'f'), ('b7e', 0x7e, 'b'),
        ('heading_414', 0x414, 'f'), ('surface_460', 0x460, 'i'), ('surface_464', 0x464, 'i'),
    ]
    LOCATE_VECS = [('loc_n', 0x14), ('loc_t', 0x20), ('node_lat_3b8', 0x3b8), ('node_up_3c4', 0x3c4),
                   ('node_fwd_3d0', 0x3d0), ('alt_side_3dc', 0x3dc), ('alt_n_3e8', 0x3e8),
                   ('alt_t_3f4', 0x3f4), ('alt_b_400', 0x400)]

    def locate_row(self):
        m = self.m
        row = []
        for name, off, kind in self.LOCATE_FIELDS:
            raw = m.get_bytes(CAR + off, 4)
            v = {'hs': lambda: struct.unpack('<h', raw[:2])[0], 'i': lambda: struct.unpack('<i', raw)[0],
                 'b': lambda: raw[0], 'h': lambda: struct.unpack('<H', raw[:2])[0],
                 'f': lambda: struct.unpack('<f', raw)[0]}[kind]()
            row.append((name, v))
        row += [('loc_poly', self.poly_index(m.get_u32(CAR + 0x30))),
                ('loc_tree', self.node_code(m.get_u32(CAR + 0x34)))]
        for name, off in self.LOCATE_VECS:
            row += [(f'{name}_{k}', v) for k, v in enumerate(m.car_get(off, 3))]
        for w in range(4):
            o = m.wheel(w) + 4
            row += [(f'w{w}_poly', self.poly_index(m.get_u32(o + 0x28))),
                    (f'w{w}_tree', self.node_code(m.get_u32(o + 0x2c))),
                    (f'w{w}_b09', m.get_bytes(o + 9, 1)[0]), (f'w{w}_b0a', m.get_bytes(o + 0xa, 1)[0]),
                    (f'w{w}_flags', struct.unpack('<H', m.get_bytes(o + 0x24, 2))[0])]
        return row

    # -- closed loop ------------------------------------------------------
    def setup_drive(self, car, node, sign, v0):
        """993 C4 (V6 geometry) on node `node`, heading `sign`·forward, at
        v0 along it, 5 cm above the support under the node centre."""
        m = self.m
        gear = self.setup(car, v0, 0.0, 1)
        nd = self.node(node)
        u = nd['up']
        f = [f32(sign * c) for c in nd['fwd']]
        r = [f32(sign * c) for c in nd['lat']]
        c = nd['pos']
        m.uc.mem_write(QOWNER, bytes(0x30))
        probe = (c[0], f32(c[1] + 1.0), c[2])
        m.put(QPOINT, *probe)
        m.call_thiscall(SELECT, QOWNER, QPOINT, 0)
        if m.get_u32(QOWNER + 0x28) == 0:
            raise RuntimeError(f'no support under node {node}')
        n = m.get(m.call_thiscall(NORMAL, QOWNER), 3)
        b = m.get(m.call_thiscall(VERTEX, QOWNER, 0), 3)
        h = f32((-((c[0] - b[0]) * n[0]) - (c[2] - b[2]) * n[2]) / n[1] + b[1])
        axes = (r, u, f)
        pos = tuple(f32(v) for v in (c[0] + 0.35 * u[0], h + 0.35 * u[1] + 0.05, c[2] + 0.35 * u[2]))
        m.car_put(0x364, *[x for a in axes for x in a])
        m.car_put(0x330, *pos)
        vel = tuple(f32(v0 * x) for x in f)
        m.car_put(0x33c, *vel)
        m.car_put(0x35c, vs.octagonal(vel[0], vel[2]))
        m.put_i32(CAR + 0x32c, len(vg.BODY_POINTS))
        for k, pt in enumerate(vg.BODY_POINTS):
            m.car_put(0x8c + 12 * k, *pt)
        m.car_put(0x3a8, *vg.HALF_EXTENTS)
        m.put_bytes(CAR + 0x7c, [1])
        m.put_i32(CAR + 0x52c, 1)                  # race participant (0x40f160)
        m.uc.mem_write(CAR + 0x8, struct.pack('<hhi', node, 0, -1))
        m.car_put(0x3e8, *n)
        m.car_put(0x400, *b)
        m.car_put(0x41c, h)
        rows = list(zip(*axes))
        for k, pt in enumerate(vg.BODY_POINTS[:4]):
            w = [f32(sum(pt[j] * rows[i][j] for j in range(3)) + pos[i]) for i in range(3)]
            m.put(m.wheel(k) + 0x40, *w)
        m.call(0x49ae30, CAR)
        m.put(vs.NORMAL, *n)
        up = f32(sum(u[i] * n[i] for i in range(3)))
        m.car_put(0x3a0, up)
        m.call(vs.GROUND_FRAME, CAR, vs.NORMAL, bits(up))
        return gear

    def controller(self, vmax, sign):
        """Steer at the node centre 6 nodes ahead; gas up to vmax (d60)."""
        m = self.m
        node = struct.unpack('<h', m.get_bytes(CAR + 8, 2))[0]
        target = self.node(self.step_node(node, 6 * sign))['pos']
        p = m.car_get(0x330, 3)
        r = m.car_get(0x364, 3)
        d = [target[i] - p[i] for i in range(3)]
        ln = math.sqrt(d[0] ** 2 + d[2] ** 2) or 1.0
        s = (d[0] * r[0] + d[2] * r[2]) / ln
        steer = max(-126, min(126, int(round(s * 400))))
        gas = 254 if m.car_get(0xd60)[0] < vmax else 0
        return word(gas=gas, steer=steer)

    def drive(self, track, name, car, node, sign, v0, vmax, steps):
        m = self.m
        run = f'{track}/{name}'
        gear = self.setup_drive(car, node, sign, v0)
        init = [('run', run), ('track', track), ('car', car), ('node0', node), ('sign', sign),
                ('v0', f32(v0)), ('gear0', gear), ('rpm0', self.f(0xdb4)), ('steps', steps),
                ('sched0', vs.SCHED0), ('rand_state0', m.get_u32(vs.G_RAND_STATE))]
        init += self.ground_row('init_') + self.read(CAR, vg.INPUT_FIELDS, 'init_')
        body, dyn = [], []
        self.written = set()
        self.tracking = True
        for n in range(steps):
            s = vs.SCHED0 + n
            m.put_u32(vs.G_SCHED, s)
            if s % 2 == 0:
                m.call(TRACK_LOCATE, CAR)
                w = self.controller(vmax, sign)
                m.put_u32(vs.G_INPUT_WORD, w)
                m.call(vs.READ_INPUTS, CAR)
                m.call(vs.DYNAMICS_TICK, CAR)
                if not self.tick():
                    raise RuntimeError(f'{run} step {n}: reached {sorted(map(hex, self.reached))}')
                dyn.append(self.dyn_row(run, n, w) + self.ground_row(full=False) + self.locate_row())
            m.call(vs.INTEGRATE_POS, CAR)
            m.call(vs.INTEGRATE_ORI, CAR)
            body.append(self.body_row(run, n))
        self.tracking = False
        bad = {a for a in self.written - self.allowed if not HEAP <= a < HEAP + HEAP_SIZE}
        if bad:
            raise RuntimeError(f'{run}: unexpected writes {sorted(hex(a) for a in bad)[:20]}')
        return init, body, dyn


# -- query streams ----------------------------------------------------------
def query_streams(polygons, rec, rng, track):
    """(stream, [(point, reuse)]) lists."""
    streams = []

    def interior(pts):
        w = [rng.random() for _ in pts]
        s = sum(w)
        return [sum(w[k] * pts[k][c] for k in range(len(pts))) / s for c in range(3)]

    for k in range(150):
        pts, _ = polygons[rng.randrange(len(polygons))]
        p = interior(pts)
        streams.append((f'interior{k}', [((f32(p[0]), f32(p[1] + rng.uniform(-2, 2)), f32(p[2])), 1)]))
    for k in range(60):
        pts, _ = polygons[rng.randrange(len(polygons))]
        v = pts[rng.randrange(len(pts))]
        streams.append((f'vertex{k}', [((v[0], f32(v[1] + 0.5), v[2]), 1)]))
        a, b = pts[0], pts[1]
        mid = tuple(f32((a[c] + b[c]) / 2) for c in range(3))
        streams.append((f'edge{k}', [((mid[0], f32(mid[1] + 0.5), mid[2]), 1)]))
    xs = [p[0] for pts, _ in polygons for p in pts]
    zs = [p[2] for pts, _ in polygons for p in pts]
    ys = [p[1] for pts, _ in polygons for p in pts]
    for k in range(40):
        p = (f32(rng.uniform(min(xs), max(xs))), f32(rng.uniform(min(ys), max(ys))),
             f32(rng.uniform(min(zs), max(zs))))
        streams.append((f'box{k}', [(p, 1)]))
    # Walks: along the node centre line, 0.5 m steps, 1 m above, reuse 1
    # and the same points with reuse 0 (fresh tree query each time).
    for k in range(6):
        start = rng.randrange(max(1, rec.node_count - 40))
        pts = []
        for j in range(30):
            a, b = rec.node(start + j)['pos'], rec.node(start + j + 1)['pos']
            for t in range(12):
                u = t / 12
                pts.append(tuple(f32(a[c] + (b[c] - a[c]) * u + (1.0 if c == 1 else 0.0)) for c in range(3)))
        lat = rec.node(start)['lat']
        off = rng.uniform(-4, 4)
        pts = [(f32(p[0] + off * lat[0]), p[1], f32(p[2] + off * lat[2])) for p in pts]
        streams.append((f'walk{k}_reuse', [(p, 1) for p in pts]))
        streams.append((f'walk{k}_fresh', [(p, 0) for p in pts]))
    return streams


def record_track(rec, track, polygons, drives):
    rng = random.Random(sum(map(ord, track)))
    qrows = []
    for name, seq in query_streams(polygons, rec, rng, track):
        rec.m.uc.mem_write(QOWNER, bytes(0x30))
        for i, (p, reuse) in enumerate(seq):
            row = [('track', track), ('stream', name), ('seq', i), ('reuse', reuse),
                   ('px', p[0]), ('py', p[1]), ('pz', p[2])]
            qrows.append(row + rec.query(p, reuse))
    runs, body, dyn = [], [], []
    for name, node, sign, v0, vmax, steps in drives:
        r, b, d = rec.drive(track, name, '993C4', node, sign, v0, vmax, steps)
        runs.append(r)
        body += b
        dyn += d
    return qrows, runs, body, dyn


def nearest_node(rec, x, z):
    best = None
    for k in range(rec.node_count):
        p = rec.node(k)['pos']
        d = (p[0] - x) ** 2 + (p[2] - z) ** 2
        if best is None or d < best[0]:
            best = (d, k)
    return best[1]


# Drives: (name, start node or (x, z) original axes, direction, v0, vmax, 64 Hz steps).
DRIVES = {
    # The ramp the user reported (scene (187.4, 12.6, -118.1) at the top),
    # driven down at 35 m/s.
    'autobahn': [('ramp_down', (187.36, 118.12), None, 33.0, 35.0, 448),
                 ('start', 2, 1, 15.0, 30.0, 512)],
    'alps': [('start', 2, 1, 15.0, 30.0, 512)],
    'monaco1': [('start', 2, 1, 12.0, 25.0, 512)],
    'canyon': [('start', 2, 1, 15.0, 30.0, 384)],
}


def record(game_dir, poly_dir, tracks):
    rec = SupportRecorder(game_dir)
    out = {'query': [], 'runs': [], 'body': [], 'dyn': []}
    for track in tracks:
        polygons = load_polygons(poly_dir / f'{track}-polygons.tsv')
        crp = ah.track_crp(game_dir / 'GameData/Track' / f'{track}.crp')
        drives = []
        rec.build_support(polygons)
        rec.build_nodes(crp)
        for name, node, sign, v0, vmax, steps in DRIVES.get(track, []):
            if isinstance(node, tuple):
                k = nearest_node(rec, *node)
                # Downhill: the direction whose node 6 ahead is lower.
                ya = rec.node(rec.step_node(k, 6))['pos'][1]
                yb = rec.node(rec.step_node(k, -6))['pos'][1]
                node, sign = k, (1 if ya < yb else -1)
            drives.append((name, node, sign, v0, vmax, steps))
        q, r, b, d = record_track(rec, track, polygons, drives)
        out['query'] += q
        out['runs'] += r
        out['body'] += b
        out['dyn'] += d
        print(f'{track}: {len(polygons)} polygons, {len(q)} queries, {len(b)} loop steps', flush=True)
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--game-dir', type=Path, default=vh.ROOT / 'local/game')
    parser.add_argument('--polygons', type=Path, default=vh.ROOT / 'local/derived/support')
    parser.add_argument('--out-dir', type=Path, required=True)
    parser.add_argument('--tracks', default='autobahn,alps,monaco1,canyon')
    parser.add_argument('--once', action='store_true', help='skip the second (determinism) run')
    args = parser.parse_args()
    tracks = args.tracks.split(',')
    first = record(args.game_dir, args.polygons, tracks)
    if not args.once:
        second = record(args.game_dir, args.polygons, tracks)
        if first != second:
            raise RuntimeError('two runs differ')
    note = 'generated by scripts/research/replay-road-support.py (x87 control word 0x27f).'
    o = args.out_dir
    vs.write_tsv(o / 'support-query.tsv', first['query'], [
        'Owner methods 0x474060/0x474420/0x474430/0x4745f0 on the real support tree; ' + note])
    vs.write_tsv(o / 'support-loop-runs.tsv', first['runs'], [
        'Closed-loop drives on real tracks with Car_TrackLocate and the real support; ' + note])
    vs.write_tsv(o / 'support-loop-body.tsv', first['body'], ['State after each 64 Hz step; ' + note])
    vs.write_tsv(o / 'support-loop-dyn.tsv', first['dyn'], [
        'Dyn, ground and locator fields after each 32 Hz step; ' + note])
    print(f"{len(first['query'])} queries, {len(first['runs'])} runs, {len(first['body'])} steps -> {o}")


if __name__ == '__main__':
    main()

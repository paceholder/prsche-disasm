"""Record the original car damage morph of nfs5.exe (R3DCar_UpdateDamage 0x438230).

Runs unchanged in Unicorn (career_harness.CareerMachine): R3DCar_UpdateDamage 0x438230 (zone
cache `R3DCar+0x6a8`), the capped zone read R3DCar_LightCap 0x4382e0 (cap tables 0x5b35ac /
0x5b35d4, viewed car 0x465d80, camera id 0x465770), the change test 0x438390, the per-LOD morph
R3DCarLOD_ApplyDamageMorph 0x43e390 with the CRP lookups 0x59f2a0 / 0x59f7c0 / 0x59f5e0 /
0x43c6f0 / 0x43c720 / 0x43c6d0, the scale 0x53c5d0 and the base copy 0x5323e0 -> 0x5b0000.

Built by hand, as R3DCarLOD init 0x43c1b0 lays them out: one R3DCarLOD per level ({vtable,
owner R3DCar, level, parts, count}), and per article that lists the level in its Base level
table a 0x54-byte part record: +0x00 article header, +0x04 the lookup's not-found end,
+0x08/+0x0c lookup cache, +0x40 the drawn vertex array (initialised with a copy of `vt`
level, **Hypothesis**: what the LOD build leaves there), +0x44 = Base+0x44, +0x48 = the Base
level entry (u16 level at +8), +0x50 heavy-damage byte. `__RTDynamicCast` 0x5a134b returns its
argument; the damage textures R3DCar_ApplyDamageTextures path 0x4451e0 is a counting stub.

Inputs per step: the ten zones `car+0x4e4`, the viewed car (`car+0x520` vs `[0x65741c]`,
`[0x628c70]` = 0), the camera id `[0x627f3c]`, the camera flags `[0x627f43]` and the car
detail `[0x6197c4]`.

    PYTHON scripts/research/replay-car-deform.py --game-dir local/game --data synthetic \
        --out iterations/012-campaign-fidelity/crates/nfs-assets/tests/fixtures/car-deform-synthetic.tsv
    PYTHON scripts/research/replay-car-deform.py --game-dir local/game --data local/game \
        --cars 356a,911gt1,959 --out local/fixtures/car-deform-cars.tsv

Format (tab separated; floats as f32 bit patterns in hex):
    crp   <hex of the synthetic decompressed CRP>         (synthetic only)
    car   <name>                                          (game data: GameData/CarModel/<name>.crp)
    part  <k> <article> <level> <vertex count>
    step  <n> <car id> <viewed id> <camera id> <flags43> <detail> <z0..z9> <cache0..9> <heavy> <tex calls>
    buf   <n> <k> <FNV-1a 64 of the part's 16-byte vertex array>   (parts changed by step n)
"""
import argparse
import random
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ai_harness import refpack  # noqa: E402
from career_harness import CareerMachine, fnv64  # noqa: E402
from vehicle_harness import CAR, load_module  # noqa: E402

UPDATE = 0x438230
TEXTURES = 0x4451e0
G_DETAIL, G_VIEWED, G_VIEW_OBJ = 0x6197c4, 0x65741c, 0x628c70
G_CAMERA_ID, G_CAMERA_FLAGS = 0x627f3c, 0x627f43
CPU_COPY_FLAGS = (0x5deb1c, 0x5deb18, 0x5deb10, 0x5deb30)   # 0x5323e0 picks the plain copy
PART = 0x54
VT, ZD, ARTI, BASE = 0x76740000, 0x647a0000, 0x41727469, 0x42617365


def bits(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


# -- CRP access (same rules as 0x59f7c0 / 0x43c6d0) ---------------------------------------
def header(crp, o):
    return struct.unpack_from('<IIII', crp, o)


def children(crp, o):
    _i, lf, count, rel = header(crp, o)
    first = o + rel * 16 if lf & 2 else rel
    first += (lf >> 5) * 16
    return [first + 16 * k for k in range(count)], first + 16 * count


def payload(crp, o):
    _i, lf, count, rel = header(crp, o)
    return (o + rel if lf & 2 else rel), lf >> 8, count


def articles(crp):
    ac = struct.unpack_from('<I', crp, 4)[0] >> 5
    start = struct.unpack_from('<I', crp, 12)[0] * 16
    return [start + 16 * k for k in range(ac)]


def child(crp, art, ident):
    kids, _end = children(crp, art)
    return next((c for c in kids if header(crp, c)[0] == ident), None)


def base_levels(crp, art):
    """[(level, entry offset in the file)] of the Base level table (`Base+0x64`, 12 bytes)."""
    b = child(crp, art, BASE)
    o, _size, _n = payload(crp, b)
    first, last = struct.unpack_from('<II', crp, o + 8)
    return [(struct.unpack_from('<H', crp, o + 0x64 + 12 * k + 8)[0], o + 0x64 + 12 * k)
            for k in range(first, last)], o


# -- synthetic CRP (layout only, random content) ------------------------------------------
def synthetic_crp(seed=7):
    rng = random.Random(seed)
    arts = []
    for a in range(9):
        nlevels = rng.choice((1, 2, 3))
        levels = sorted(rng.sample(range(1, 6), nlevels))
        flags = rng.choice((0x08, 0x1a, 0x1e, 0x18, 0x02)) if a else 0x02   # article 0 unflagged
        kids = {}
        base = bytearray(0x64 + 12 * nlevels)
        struct.pack_into('<II', base, 8, 0, nlevels)
        struct.pack_into('<I', base, 12, nlevels)
        base[0x44 + 10] = flags
        for k, lv in enumerate(levels):
            struct.pack_into('<H', base, 0x64 + 12 * k + 8, lv)
        kids[BASE] = (bytes(base), 0, False)
        for lv in levels:
            n = rng.randint(3, 40)
            vt = b''.join(struct.pack('<4f', *(rng.uniform(-9, 9) for _ in range(3)), 1.0)
                          for _ in range(n))
            kids[VT | lv] = (vt, n, True)
            shape = rng.random()
            if shape < 0.85:          # zd table; 0.75..0.85: zd without a damaged array
                triples, idx = [], []
                for zone in sorted(rng.sample(range(10), rng.randint(1, 5))):
                    vs = [rng.randrange(n) for _ in range(rng.randint(1, min(n, 12)))]
                    triples.append((zone, len(vs), len(idx)))
                    idx += vs
                zd = struct.pack('<I', len(triples)) + b''.join(struct.pack('<3I', *t) for t in triples)
                zd += b''.join(struct.pack('<I', v) for v in idx)
                kids[ZD | lv] = (zd, len(triples), True)
                if shape < 0.75:
                    dv = b''.join(struct.pack('<4f', *(rng.uniform(-2, 2) for _ in range(3)),
                                              rng.uniform(0, 1)) for _ in range(n))
                    kids[VT | lv | 0x8000] = (dv, n, True)
        arts.append(kids)
    # Layout: file header, article headers, then per article its sorted child headers and data.
    ac = len(arts)
    out = bytearray(16 + 16 * ac)
    out[0:16] = b' raC' + struct.pack('<III', ac << 5, 0, 1)
    for a, kids in enumerate(arts):
        ho = 16 + 16 * a
        ids = sorted(kids)
        table = (len(out) + 15) & ~15
        out += bytes(table - len(out)) + bytes(16 * len(ids))
        struct.pack_into('<IIII', out, ho, ARTI, 0x1a, len(ids), (table - ho) // 16)
        for k, ident in enumerate(ids):
            data, count, indexed = kids[ident]
            do = (len(out) + 15) & ~15
            out += bytes(do - len(out)) + data
            eo = table + 16 * k
            lf = (len(data) << 8) | 0x0a | (1 if indexed else 0)
            struct.pack_into('<IIII', out, eo, ident, lf, count, do - eo)
    return bytes(out + bytes((-len(out)) % 16))


# -- machine ------------------------------------------------------------------------------
class DeformMachine(CareerMachine):
    def __init__(self, module, data):
        super().__init__(module, data)
        # Admit PACODE (0x5b0000..): the base copy 0x5323e0 dispatches to its plain x86 copy
        # 0x5b0000 when the CPU feature words are 0 (as ai_harness.py).
        base = int(module['image_base'], 16)
        pacode = next(s for s in module['sections'] if s['name'] == 'PACODE')
        self.code = (self.code[0], base + pacode['rva'] + pacode['vsize'])
        self.stubs[0x5a134b] = lambda m: m.arg(0)       # R3DObject->R3DCar, R3DLOD->R3DCarLOD
        self.tex_calls = 0
        self.stubs[TEXTURES] = self._textures

    def _textures(self, _m):
        self.tex_calls += 1
        return 0

    def setup(self, crp):
        self.heap = 0x4000000
        self.tex_calls = 0
        p = self.alloc(len(crp))
        self.put_bytes(p, crp)
        self.uc.mem_write(CAR, bytes(0x1300))
        car3d = self.alloc(0x800)
        self.put_u32(car3d + 0xf4, CAR)
        parts = []               # (k, article index, level, count, record address)
        lods = []
        levels = sorted({lv for art in articles(crp) for lv, _ in base_levels(crp, art)[0]})
        for lv in levels:
            recs = []
            for ai, art in enumerate(articles(crp)):
                table, bo = base_levels(crp, art)
                entry = next((e for l2, e in table if l2 == lv), None)
                vt = child(crp, art, VT | lv)
                if entry is None or vt is None:
                    continue
                vo, _size, n = payload(crp, vt)
                buf = self.alloc(16 * n)
                self.put_bytes(buf, crp[vo:vo + 16 * n])
                _kids, end = children(crp, art)
                r = bytearray(PART)
                struct.pack_into('<II', r, 0, p + art, p + end)
                struct.pack_into('<II', r, 8, 0, 0)
                struct.pack_into('<III', r, 0x40, buf, p + bo + 0x44, p + entry)
                recs.append((ai, lv, n, buf, bytes(r)))
            block = self.alloc(PART * max(1, len(recs)))
            for k, (ai, lv2, n, buf, r) in enumerate(recs):
                self.put_bytes(block + PART * k, r)
                parts.append((len(parts), ai, lv2, n, block + PART * k, buf))
            lod = self.alloc(0x24)
            self.put_u32(lod + 4, car3d)
            self.put_u32(lod + 8, lv)
            self.put_u32(lod + 0xc, block)
            self.put_u32(lod + 0x10, len(recs))
            lods.append(lod)
        vec = self.alloc(4 * len(lods))
        for k, lod in enumerate(lods):
            self.put_u32(vec + 4 * k, lod)
        self.put_u32(car3d + 0xb0, vec)
        self.put_u32(car3d + 0xb4, vec + 4 * len(lods))
        for g in CPU_COPY_FLAGS:
            self.put_u32(g, 0)
        self.put_u32(G_VIEW_OBJ, 0)
        self.car3d, self.parts = car3d, parts

    def step(self, zones, car_id, viewed, camera, flags, detail):
        for k, z in enumerate(zones):
            self.put(CAR + 0x4e4 + 4 * k, z)
        self.put_u32(CAR + 0x520, car_id)
        self.put_u32(G_VIEWED, viewed)
        self.put_bytes(G_CAMERA_ID, struct.pack('<h', camera))
        self.put_bytes(G_CAMERA_FLAGS, bytes([flags]))
        self.put_u32(G_DETAIL, detail)
        before = self.tex_calls
        self.call_thiscall(UPDATE, self.car3d, 0, budget=50_000_000)
        cache = self.get(self.car3d + 0x6a8, 10)
        heavy = ''.join(str(self.get_bytes(rec + 0x50, 1)[0]) for _k, _a, _l, _n, rec, _b in self.parts)
        bufs = [fnv64(self.get_bytes(buf, 16 * n)) for _k, _a, _l, n, _r, buf in self.parts]
        return cache, heavy, self.tex_calls - before, bufs


def schedule(rng, n=48):
    """Zone sets: rising damage, exact t = 0.5 / 1 edges, repairs, view and detail cases."""
    z = [0.0] * 10
    out = []
    edges = (25.0, 25.000002, 50.0, 49.99, 100.0, 0.0, 12.5, 75.0, 30.0)
    for s in range(n):
        if s in (20, 40):
            z = [0.0] * 10                                   # repair
        else:
            for k in rng.sample(range(10), rng.randint(1, 4)):
                z[k] = rng.choice(edges) if rng.random() < 0.4 else \
                    struct.unpack('<f', struct.pack('<f', rng.uniform(0, 100)))[0]
        if s in (5, 6, 7, 33):
            car_id, viewed = 7, 7
            camera, flags = (0, 8) if s in (5, 33) else (1, 8) if s == 6 else (3, 0)
        elif s == 8:
            car_id, viewed, camera, flags = 7, 7, 0, 0           # camera id 0 without the in-car flag
        else:
            car_id, viewed, camera, flags = 3, 7, 3, 0
        detail = 0 if s in (12, 13) else 2
        out.append((list(z), car_id, viewed, camera, flags, detail))
    return out


def record(m, crp, rng, lines):
    m.setup(crp)
    for k, ai, lv, n, _rec, _buf in m.parts:
        lines.append(f'part\t{k}\t{ai}\t{lv}\t{n}')
    prev = [None] * len(m.parts)
    for s, (zones, car_id, viewed, camera, flags, detail) in enumerate(schedule(rng)):
        cache, heavy, tex, bufs = m.step(zones, car_id, viewed, camera, flags, detail)
        cols = [str(s), str(car_id), str(viewed), str(camera), str(flags), str(detail)]
        cols += [f'{bits(z):08x}' for z in zones] + [f'{bits(c):08x}' for c in cache]
        lines.append('step\t' + '\t'.join(cols + [heavy, str(tex)]))
        for k, h in enumerate(bufs):
            if h != prev[k]:
                lines.append(f'buf\t{s}\t{k}\t{h:016x}')
        prev = bufs


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--game-dir', type=Path)
    ap.add_argument('--data', required=True, help="'synthetic' or a game dir with GameData/CarModel")
    ap.add_argument('--cars', default='356a')
    ap.add_argument('--seed', type=int, default=3)
    ap.add_argument('--out', type=Path, required=True)
    a = ap.parse_args()
    module, data = load_module(game_dir=a.game_dir)
    m = DeformMachine(module, data)
    lines = [f'# {line}' for line in __doc__.strip().splitlines()[:1]]
    if a.data == 'synthetic':
        crp = synthetic_crp(a.seed)
        lines.append('crp\t' + crp.hex())
        record(m, crp, random.Random(a.seed), lines)
    else:
        for car in a.cars.split(','):
            raw = (Path(a.data) / 'GameData/CarModel' / f'{car}.crp').read_bytes()
            crp = refpack(raw) if raw[1] == 0xfb and raw[0] & 0xfe == 0x10 else raw
            lines.append(f'car\t{car}')
            record(m, crp, random.Random(f'{a.seed}:{car}'), lines)
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'wrote {a.out} ({len(lines)} lines)')


if __name__ == '__main__':
    main()
